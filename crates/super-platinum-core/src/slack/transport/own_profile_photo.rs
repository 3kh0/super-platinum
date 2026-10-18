//! Closed one-shot profile photo transport. No generic RequestBody extension.
use super::*;
use crate::slack::api::photo;
impl Transport {
    /// Only a validated preparePhoto result can name this download. Keep local
    /// and prepared-image capacities equal; never forward cookies on redirects.
    pub async fn read_prepared_own_photo(
        &self,
        prepared: &photo::PreparedPhoto,
    ) -> Result<Vec<u8>, Error> {
        if self.health() == Health::Offline {
            return Err(Error::Offline(
                "Photo preview was not requested while offline".into(),
            ));
        }
        self.http()?;
        let response = self
            .single_attempt_client()?
            .get(prepared.url())
            .timeout(MEDIA_FETCH_TIMEOUT)
            .header("Cookie", self.cookie())
            .header(
                "User-Agent",
                crate::slack::xparams::Identity::from_capture().user_agent,
            )
            .send()
            .await
            .map_err(|_| Error::Transport("Prepared photo could not be loaded".into()))?;
        let retry = retry_after_secs(response.headers());
        if response.status().as_u16() == 429 {
            return Err(Error::RateLimited {
                retry_after_secs: retry,
            });
        }
        if !response.status().is_success() {
            return Err(Error::HttpStatus {
                status: response.status().as_u16(),
                retry_after_secs: retry,
            });
        }
        read_media_body(response, photo::MAX_BYTES)
            .await
            .map_err(|_| {
                Error::Transport(
                    "Prepared photo was incomplete or exceeded preview capacity".into(),
                )
            })
    }
    pub async fn prepare_own_photo(
        &self,
        upload: photo::Upload,
    ) -> Result<photo::PreparedPhoto, Error> {
        origin(&upload.request)?;
        if self.health() == Health::Offline {
            return Err(Error::Offline(
                "Photo upload was not sent while offline".into(),
            ));
        }
        let http = self.single_attempt_client()?;
        self.http()?;
        let boundary = format!("super-platinum-photo-{}", uuid::Uuid::new_v4().simple());
        let (prefix, image, suffix) = multipart_parts(&upload, &boundary);
        let length = prefix.len() + image.len() + suffix.len();
        let chunks = futures::stream::once(async move { Ok::<_, std::io::Error>(prefix) })
            .chain(futures::stream::unfold(
                (image, 0usize),
                |(bytes, offset)| async move {
                    if offset == bytes.len() {
                        return None;
                    }
                    let end = (offset + 64 * 1024).min(bytes.len());
                    Some((
                        Ok::<_, std::io::Error>(bytes[offset..end].to_vec()),
                        (bytes, end),
                    ))
                },
            ))
            .chain(futures::stream::once(async move {
                Ok::<_, std::io::Error>(suffix)
            }));
        let body = wreq::Body::wrap_stream(chunks);
        let mut builder = http.post(&upload.request.url).timeout(API_TIMEOUT);
        for (k, v) in &upload.request.headers {
            if !k.eq_ignore_ascii_case("content-type") {
                builder = builder.header(k.as_str(), v.as_str());
            }
        }
        let response = builder
            .header("Cookie", self.cookie())
            .header(
                "Content-Type",
                format!("multipart/form-data; boundary={boundary}"),
            )
            .header("Content-Length", length.to_string())
            .body(body)
            .send()
            .await
            .map_err(|_| Error::Transport("Photo preparation response was lost".into()))?;
        self.set_health(Health::Online);
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
        let bytes = read_media_body(response, 64 * 1024).await?;
        let value = serde_json::from_slice(&bytes)
            .map_err(|_| Error::Transport("Unreadable prepared photo response".into()))?;
        photo::PreparedPhoto::parse(&value)
    }
    pub async fn mutate_own_photo(&self, mutation: photo::Mutation) -> Result<(), Error> {
        origin(&mutation.request)?;
        if self.health() == Health::Offline {
            return Err(Error::Offline(
                "Photo change was not sent while offline".into(),
            ));
        }
        let value = self
            .execute_once_using(&mutation.request, true, self.single_attempt_client()?)
            .await
            .map_err(|e| match e {
                Error::Offline(_) => Error::Transport("Photo response lost after dispatch".into()),
                e => e,
            })?;
        photo::confirm(&value)
    }
}
fn origin(req: &PreparedRequest) -> Result<(), Error> {
    let valid = url::Url::parse(&req.url).is_ok_and(|u| {
        let origin = u.scheme() == "https"
            && u.host_str().is_some_and(|h| h.ends_with(".slack.com"))
            && u.port_or_known_default() == Some(443);
        #[cfg(test)]
        let origin = origin || (u.scheme() == "http" && u.host_str() == Some("127.0.0.1"));
        origin
            && u.username().is_empty()
            && u.password().is_none()
            && u.fragment().is_none()
            && req.method == "POST"
            && matches!(
                u.path(),
                "/api/users.preparePhoto" | "/api/users.setPhoto" | "/api/users.deletePhoto"
            )
    });
    if valid {
        Ok(())
    } else {
        Err(Error::Api("invalid_own_photo_destination".into()))
    }
}
fn multipart_parts(upload: &photo::Upload, boundary: &str) -> (Vec<u8>, Arc<[u8]>, Vec<u8>) {
    let RequestBody::Form(fields) = &upload.request.body else {
        unreachable!("closed builder");
    };
    let mut body = Vec::new();
    for (key, value) in fields {
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"{key}\"\r\n\r\n{value}\r\n"
            )
            .as_bytes(),
        );
    }
    body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"image\"; filename=\"profile-photo\"\r\nContent-Type: {}\r\n\r\n",upload.image.mime()).as_bytes());
    (
        body,
        upload.image.bytes(),
        format!("\r\n--{boundary}--\r\n").into_bytes(),
    )
}
#[cfg(test)]
fn multipart(upload: &photo::Upload, boundary: &str) -> Vec<u8> {
    let (mut prefix, image, suffix) = multipart_parts(upload, boundary);
    prefix.extend_from_slice(&image);
    prefix.extend_from_slice(&suffix);
    prefix
}
#[cfg(test)]
mod tests;
