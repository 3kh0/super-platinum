//! Shared attachment viewer for conversation attachments and Files.
use crate::media::MediaUri;
use crate::state::{ShellState, ViewerVm};
use dioxus::prelude::*;
use super_platinum_core::{MediaAssetId, file_preview};

#[component]
pub fn FileViewer(state: Signal<ShellState>, viewer: ViewerVm, media_epoch: u64) -> Element {
    let (ready, failed) = {
        let shell = state.peek();
        (
            shell.media.is_ready(&viewer.id),
            shell.media.image_state(&viewer.id).1,
        )
    };
    // Playback mounts only after bytes are ready. Unrelated media arrivals must
    // not replace the source and restart playback.
    let uri = viewer.id.webview_uri();
    let actions = crate::file_actions::Target::for_viewer(&state.read(), &viewer);
    let has_actions = actions.is_some();
    rsx! {
        div {class:"viewer",
            if !ready {
                if failed {p {class:"media-unavailable",role:"status","Preview unavailable"}}
                else {p {class:"viewer-loading",role:"status","Loading {viewer.name}…"}}
            } else if viewer.mime.starts_with("image/") {
                crate::view::media_image::MediaImage {state,id:viewer.id.clone(),media_epoch,alt:viewer.name.clone()}
            } else if viewer.mime.starts_with("video/") {
                video {key:"viewer-{uri}",src:"{uri}",controls:true,autoplay:true,"{viewer.name}"}
            } else if viewer.mime.starts_with("audio/") {
                audio {class:"audio-preview",src:"{uri}",controls:true,preload:"metadata","aria-label":"Play {viewer.name}","{viewer.name}"}
            } else if viewer.mime == "application/pdf" {
                crate::pdf_preview::PdfPreview {key:"{viewer.id.uri()}",state,id:viewer.id.clone()}
            } else if file_preview::is_text(&viewer.mime) {
                TextContent {key:"{viewer.id.uri()}",state,id:viewer.id.clone()}
            } else {
                p {"A preview is not available for this file type. Download it to open it."}
            }
            div {class:if has_actions{"viewer-footer has-actions"}else{"viewer-footer"},
                crate::download::DownloadButton {key:"{viewer.id.uri()}",state,id:viewer.id.clone(),name:viewer.download_name.clone(),ready}
                if let Some(target)=actions {
                    crate::file_actions::Actions {key:"{target.epoch}-{target.team}-{target.id}",state,target,in_viewer:true}
                }
            }
        }
    }
}

#[component]
fn TextContent(state: Signal<ShellState>, id: MediaAssetId) -> Element {
    // Decode a bounded prefix once per opened, ready asset. Unrelated shell ticks
    // must not scan or copy the original file on every render.
    let preview = use_hook(|| {
        state
            .peek()
            .media
            .ready_bytes(&id)
            .ok_or("The preview is no longer available. Reopen the file to try again.")
            .and_then(|bytes| file_preview::decode(&bytes))
            .map(std::sync::Arc::new)
    });
    let mut wrap = use_signal(|| false);
    match preview {
        Ok(preview) => rsx! {
            div {class:"text-preview",
                div {class:"text-preview-toolbar",
                    span {"{preview.lines} " if preview.lines == 1 {"line"} else {"lines"}}
                    button {r#type:"button","aria-pressed":wrap(),onclick:move |_|wrap.toggle(),"Wrap lines"}
                }
                if preview.text.is_empty() {p {class:"text-preview-empty","This file is empty."}}
                else {pre {class:if wrap(){"text-preview-content wrap"}else{"text-preview-content"},tabindex:"0","aria-label":"File contents",code {"{preview.text}"}}}
                if preview.truncated {p {class:"text-preview-notice",role:"status","Preview shortened for this large file. Download the original to read everything."}}
            }
        },
        Err(message) => rsx! {p {class:"media-unavailable",role:"status","{message}"}},
    }
}

pub fn fixture(shell: &mut ShellState, name: &str) {
    use super_platinum_core::MediaAssetKind;
    if name.starts_with("file-preview-renamed") {
        renamed_fixture(shell, name);
        return;
    }
    let text = "// Release notes · 日本語 🦊\n<script>window.previewExecuted=true</script>\n<img src=x onerror=alert(1)>\n\tconst release = { ready: true };\n";
    let (filename, mime, bytes) = match name {
        "file-preview-audio" => {
            let samples = 8_000u32;
            let mut wav = Vec::new();
            wav.extend_from_slice(b"RIFF");
            wav.extend_from_slice(&(36 + samples * 2).to_le_bytes());
            wav.extend_from_slice(b"WAVEfmt ");
            wav.extend_from_slice(&16u32.to_le_bytes());
            wav.extend_from_slice(&1u16.to_le_bytes());
            wav.extend_from_slice(&1u16.to_le_bytes());
            wav.extend_from_slice(&8_000u32.to_le_bytes());
            wav.extend_from_slice(&16_000u32.to_le_bytes());
            wav.extend_from_slice(&2u16.to_le_bytes());
            wav.extend_from_slice(&16u16.to_le_bytes());
            wav.extend_from_slice(b"data");
            wav.extend_from_slice(&(samples * 2).to_le_bytes());
            wav.resize(44 + samples as usize * 2, 0);
            ("audio-note.wav", "audio/wav", wav)
        }
        "file-preview-empty" => ("empty.txt", "text/plain", Vec::new()),
        "file-preview-binary" => ("binary.txt", "text/plain", b"binary\0data".to_vec()),
        "file-preview-large" => (
            "large-log.txt",
            "text/plain",
            "2026-10-12: build verified\n".repeat(4_000).into_bytes(),
        ),
        "file-preview-utf16" => {
            let mut bytes = vec![0xff, 0xfe];
            for unit in text.encode_utf16() {
                bytes.extend_from_slice(&unit.to_le_bytes());
            }
            ("unicode.txt", "text/plain", bytes)
        }
        _ => (
            "release-notes.html",
            "text/html",
            format!("{text}{}", "long_line_".repeat(100)).into_bytes(),
        ),
    };
    let id = MediaAssetId::new(MediaAssetKind::Attachment);
    shell.media.insert(id.clone(), mime, bytes);
    shell.viewer = Some(ViewerVm {
        file: None,
        id,
        name: filename.into(),
        download_name: filename.into(),
        mime: mime.into(),
    });
    shell.overlay = Some(crate::state::Overlay::Viewer);
}

// Exercise both production entry points with a Slack title that is not a filename.
fn renamed_fixture(shell: &mut ShellState, name: &str) {
    use super_platinum_core::{MediaAssetKind, files::FilesPage, slack::models::File};
    let file = File {
        id: Some("FRENAMED".into()),
        title: Some("Release notes".into()),
        name: Some("release-notes-日本語.html".into()),
        mimetype: Some("text/html".into()),
        url_private: Some("https://files.slack.com/fixture/renamed-original".into()),
        ..Default::default()
    };
    let id = shell.media.register_deferred(
        MediaAssetKind::Attachment,
        file.full_url().unwrap(),
        "text/html",
    );
    shell.media.insert(
        id,
        "text/html",
        b"Original file contents remain untouched.".as_slice(),
    );
    if name == "file-preview-renamed-files" {
        shell.main_view = crate::state::MainView::Files;
        let team = shell.core.active_team.clone().unwrap();
        shell
            .core
            .files
            .ensure_scope(shell.core.account_epoch, &team);
        let generation = shell.core.files.begin(Default::default(), 1);
        shell.core.files.apply(
            generation,
            FilesPage {
                items: vec![file],
                ..Default::default()
            },
        );
    } else {
        let node = crate::message_vm::file_node(&shell.media, &file);
        if let Some(message) = shell.messages.last_mut() {
            message.body = vec![crate::state::RichNode::Paragraph(vec![node])];
        }
        shell.sync_fixture_messages();
    }
}

pub async fn native_check(mut state: Signal<ShellState>) {
    if !crate::fixture::is_fixture()
        || std::env::var_os("SUPER_PLATINUM_FILE_PREVIEW_CHECK").is_none()
    {
        return;
    }
    if state.peek().core.transport.is_some() || state.peek().core.session.is_some() {
        eprintln!("super-platinum: file preview check failed: connected fixture");
        return;
    }
    let mut check = document::eval(
        r#"
        await (async()=>{
            // Keep the eval alive until Rust consumes the final receipt.
            const report=async value=>{dioxus.send(value);await dioxus.recv();};
            const fixture=await dioxus.recv();
            const pause=()=>new Promise(r=>setTimeout(r,40));
            const assert=(c,m)=>{if(!c)throw Error(m);};
            try {
                if(fixture.startsWith('file-preview-renamed')){
                    const selector=fixture.endsWith('-files')?'.files-row':'.file-link';
                    const find=()=>Array.from(document.querySelectorAll(selector)).find(e=>e.textContent.includes('Release notes'));
                    for(let i=0;i<100&&!find();i++)await pause();
                    assert(find(),'renamed file entry rendered');find().click();
                }
                for(let i=0;i<100&&!document.querySelector('.viewer');i++)await pause();
                const viewer=document.querySelector('.viewer');
                assert(viewer,'viewer loaded');
                const title=document.querySelector('#shell-overlay-dialog h2').textContent;
                assert(!viewer.querySelector('script,img'),'text does not create active elements');
                assert(!window.previewExecuted,'text script inert');
                assert(!viewer.querySelector('[aria-label^="Download"]').disabled,'original remains downloadable');
                if(fixture.startsWith('file-preview-renamed')){
                    assert(title==='Release notes','Slack title remains in viewer');
                    assert(viewer.querySelector('[aria-label="Download release-notes-日本語.html"]'),'original filename and extension reach Download');
                    assert(viewer.querySelector('pre').textContent==='Original file contents remain untouched.','original file bytes preserved');
                }else if(title==='audio-note.wav'){
                    const audio=viewer.querySelector('audio');assert(audio&&audio.controls,'audio controls');
                    for(let i=0;i<150&&audio.readyState<1;i++)await pause();
                    assert(audio.readyState>=1&&audio.duration>0,'audio metadata decoded');
                    await audio.play();await pause();assert(!audio.paused,'audio playback starts');
                    const source=audio.currentSrc;dioxus.send({stage:'audio'});await dioxus.recv();
                    for(let i=0;i<5;i++)await pause();
                    assert(viewer.querySelector('audio')===audio&&audio.currentSrc===source&&!audio.paused,'unrelated media arrival preserves playback');
                    audio.pause();assert(audio.paused,'audio pause');
                }else if(title==='empty.txt'){
                    assert(viewer.textContent.includes('This file is empty.'),'empty file state');
                }else if(title==='binary.txt'){
                    assert(viewer.textContent.includes('binary data')&&!viewer.querySelector('pre'),'binary file rejected');
                }else{
                    const pre=viewer.querySelector('pre');assert(pre,'text preview loaded');
                    if(title==='large-log.txt'){
                        assert(viewer.querySelector('.text-preview-notice'),'large file disclosure');
                        assert(pre.textContent.split('\n').length<=2000,'bounded rendered lines');
                    }else{
                        assert(pre.textContent.includes('<script>window.previewExecuted=true</script>'),'markup preserved literally');
                        assert(pre.textContent.includes('日本語 🦊'),'unicode preserved');
                        const wrap=viewer.querySelector('.text-preview-toolbar button');wrap.click();
                        for(let i=0;i<100&&wrap.getAttribute('aria-pressed')!=='true';i++)await pause();
                        assert(pre.classList.contains('wrap')&&getComputedStyle(pre).whiteSpace==='pre-wrap','line wrapping enabled');
                    }
                }
                const rect=viewer.getBoundingClientRect();assert(rect.left>=0&&rect.right<=innerWidth,'viewer in viewport');
                assert(viewer.scrollWidth<=viewer.clientWidth+1,'viewer has no horizontal overflow');
                await report({ok:true});
            }catch(e){await report({ok:false,case:e.message});}
        })();
    "#,
    );
    let _ = check.send(std::env::var("SUPER_PLATINUM_FIXTURE").unwrap_or_default());
    let mut result = match check.recv::<serde_json::Value>().await {
        Ok(value) => value,
        Err(error) => {
            eprintln!("super-platinum: file preview check failed: evaluation {error:?}");
            return;
        }
    };
    if result["stage"] == "audio" {
        state.write().media_epoch += 1;
        let _ = check.send("media-updated");
        result = check.recv::<serde_json::Value>().await.unwrap_or_default();
    }
    let _ = check.send("received");
    if result["ok"] == true {
        eprintln!("super-platinum: file preview check passed");
    } else {
        eprintln!(
            "super-platinum: file preview check failed: {}",
            result["case"]
        );
    }
}
