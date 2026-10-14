//! Explicit disconnected WebView media protocol checks; no session/network work.
use super::MediaUri;
use crate::state::ShellState;
use dioxus::prelude::*;
use serde_json::json;
use super_platinum_core::{MediaAssetId, MediaAssetKind};

pub(crate) fn start(state: Signal<ShellState>, desktop: dioxus::desktop::DesktopContext) {
    if std::env::var("SUPER_PLATINUM_MEDIA_CHECK").as_deref() != Ok("1")
        || !crate::fixture::is_fixture()
    {
        return;
    }
    dioxus::core::spawn_forever(async move {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let media = state.peek().media.clone();
        let mut probes = Vec::new();
        let mut pending = Vec::new();
        for kind in [
            MediaAssetKind::Avatar,
            MediaAssetKind::Emoji,
            MediaAssetKind::Attachment,
            MediaAssetKind::Background,
        ] {
            for ready in [true, false] {
                let id = MediaAssetId::new(kind);
                if ready {
                    media.insert(
                        id.clone(),
                        "image/png",
                        include_bytes!("../../../assets/icons/icon-512.png").as_slice(),
                    );
                } else {
                    pending.push(id.clone());
                }
                let expected = if ready {
                    512
                } else if kind == MediaAssetKind::Emoji {
                    1
                } else {
                    64
                };
                probes.push(json!({"kind":kind.as_str(),"ready":ready,"expected":expected,"src":id.webview_uri_at(7)}));
            }
        }
        // Keep detached Image instances alive between polls. These exercise the
        // same CSP/protocol/decoder path without altering the fixture layout.
        let script = format!(
            r#"(()=>{{
            const specs={probes};
            const images=window.__spMediaProtocolCheck ||= specs.map(spec=>{{const img=new Image();img.src=spec.src;return {{...spec,img}};}});
            const values=images.map(p=>({{kind:p.kind,ready:p.ready,expected:p.expected,complete:p.img.complete,width:p.img.naturalWidth,height:p.img.naturalHeight}}));
            const dom=[...document.images].filter(i=>i.src.startsWith('super-platinum-media:')||i.src.startsWith('http://super-platinum-media.'));
            const broken=dom.filter(i=>!i.complete||i.naturalWidth===0).length;
            return {{ok:values.length===8&&values.every(p=>p.complete&&p.width===p.expected&&p.height===p.expected)&&dom.length>0&&broken===0,
                probes:values,domImages:dom.length,brokenDomImages:broken,viewport:[innerWidth,innerHeight]}};
            }})()"#,
            probes = serde_json::to_string(&probes).expect("fixture media probe JSON")
        );
        let mut receipt = crate::slack_links_check::wait_dom(&desktop, &script)
            .await
            .unwrap_or_else(|error| json!({"ok":false,"error":format!("Media protocol: {error}")}));
        if receipt["ok"] == true {
            // The exact same Image instances and opaque IDs must replace their
            // already-decoded placeholders after only the generation changes.
            let refresh: Vec<_> = pending
                .into_iter()
                .map(|id| {
                    media.insert(
                        id.clone(),
                        "image/png",
                        include_bytes!("../../../assets/icons/icon-512.png").as_slice(),
                    );
                    json!({"kind":id.kind().as_str(),"src":id.webview_uri_at(8)})
                })
                .collect();
            let script = format!(
                r#"(()=>{{
                const specs={refresh};
                const images=(window.__spMediaProtocolCheck||[]).filter(p=>!p.ready);
                for(const p of images){{const spec=specs.find(s=>s.kind===p.kind);if(spec&&p.img.src!==spec.src)p.img.src=spec.src;}}
                const values=images.map(p=>({{kind:p.kind,complete:p.img.complete,width:p.img.naturalWidth,height:p.img.naturalHeight,generation:new URL(p.img.src).searchParams.get('v')}}));
                return {{ok:values.length===4&&values.every(p=>p.complete&&p.width===512&&p.height===512&&p.generation==='8'),probes:values}};
                }})()"#,
                refresh = serde_json::to_string(&refresh).expect("fixture media refresh JSON")
            );
            match crate::slack_links_check::wait_dom(&desktop, &script).await {
                Ok(refresh) => {
                    receipt["generationRefresh"] = json!(true);
                    receipt["refreshProbes"] = refresh["probes"].clone();
                }
                Err(error) => {
                    receipt["ok"] = json!(false);
                    receipt["generationRefresh"] = json!(false);
                    receipt["error"] = json!(format!("Media generation refresh: {error}"));
                }
            }
        }
        eprintln!("media-protocol-check: {receipt}");
    });
}
