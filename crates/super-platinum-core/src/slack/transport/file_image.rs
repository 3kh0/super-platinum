//! Single attempt, exact original-owner reads. No invisible 429 replay.
use super::*;
use crate::slack::api::file_image::Request;
impl Transport {
    pub async fn execute_file_image_current(
        &self,
        request: &Request,
        current: impl Fn() -> bool,
    ) -> Result<Value, Error> {
        let retired = || Error::Offline("Image preview owner retired".into());
        if !current() {
            return Err(retired());
        }
        let work = async {
            self.await_link().await?;
            if !current() {
                return Err(retired());
            }
            let exchange = async {
                let http = self.single_attempt_client()?;
                let mut builder = http
                    .post(&request.inner.url)
                    .timeout(API_TIMEOUT)
                    .header("Cookie", self.cookie());
                for (k, v) in &request.inner.headers {
                    builder = builder.header(k.as_str(), v.as_str());
                }
                let RequestBody::Form(fields) = &request.inner.body else {
                    unreachable!()
                };
                if request.multipart {
                    let mut form = wreq::multipart::Form::new();
                    for (k, v) in fields {
                        form = form.text(k.clone(), v.clone());
                    }
                    builder = builder.multipart(form);
                } else {
                    builder = builder.form(fields);
                }
                let response = builder.send().await.map_err(send_error)?;
                let status = response.status();
                let retry = retry_after_secs(response.headers());
                if status.as_u16() == 429 {
                    return Err(Error::RateLimited {
                        retry_after_secs: retry,
                    });
                }
                if !status.is_success() {
                    return Err(Error::HttpStatus {
                        status: status.as_u16(),
                        retry_after_secs: retry,
                    });
                }
                let bytes = read_media_body(response, 4 * 1024 * 1024).await?;
                serde_json::from_slice::<Value>(&bytes)
                    .map_err(|_| Error::Api("image_preview_unavailable".into()))
            };
            tokio::select! {
                biased;
                _=self.link_dropped()=>Err(Error::Offline("Image preview link dropped".into())),
                result=exchange=>result,
            }
        };
        let result = tokio::select! {
            biased;
            _=async{loop{if !current(){break;}tokio::time::sleep(Duration::from_millis(100)).await;}}=>Err(retired()),
            result=tokio::time::timeout(API_TIMEOUT+OFFLINE_HOLD,work)=>result.unwrap_or_else(|_|Err(Error::Offline("Image preview timed out".into()))),
        };
        if !current() {
            return Err(retired());
        }
        result
    }
}
#[cfg(test)]
mod tests;
