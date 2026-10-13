//! Native details disclosure checks; no synthetic keyboard default actions.
use super::*;

pub(super) async fn run(
    desktop: &dioxus::desktop::DesktopContext,
    surface: &str,
) -> Result<Value, String> {
    let (target, content, description, label) = match surface {
        "table" => (
            ".slack-list-grid",
            ".slack-list-scroll",
            "slack-list-keyboard",
            "List keyboard shortcuts",
        ),
        "board" => (
            ".slack-list-board",
            ".slack-list-board",
            "slack-list-board-help",
            "Keyboard shortcuts",
        ),
        "record" => (
            "#slack-list-record-detail",
            ".slack-list-detail-scroll",
            "slack-list-record-help",
            "Item keyboard shortcuts",
        ),
        _ => return Err("Unknown keyboard help surface".into()),
    };
    let setup = r#"(()=>{
      const target=document.querySelector(TARGET),content=document.querySelector(CONTENT),description=document.getElementById(DESCRIPTION);
      const details=description?.nextElementSibling,summary=details?.querySelector('summary'),paragraph=details?.querySelector('p');
      const name=summary?.getAttribute('aria-label')||summary?.textContent.trim(),rect=summary?.getBoundingClientRect();
      const described=target?.getAttribute('aria-describedby')?.split(/\s+/).includes(DESCRIPTION);
      const hidden=getComputedStyle(description),text=description?.textContent.trim();
      const ok=details?.tagName==='DETAILS'&&!details.open&&summary?.tabIndex===0&&name===LABEL&&summary.textContent.trim()==='Keyboard shortcuts'&&!!summary.querySelector('svg')&&described&&text.length>80&&paragraph?.textContent.trim()===text&&hidden.display!=='none'&&description.getBoundingClientRect().height<=1&&rect.height>=24&&rect.left>=0&&rect.right<=innerWidth&&rect.top>=0&&rect.bottom<=innerHeight&&paragraph.checkVisibility()===false&&content.clientHeight>0;
      if(!ok)return {ok:false,name,described,tag:details?.tagName,open:details?.open,rect:rect?.toJSON(),contentHeight:content?.clientHeight,tabIndex:summary?.tabIndex,summaryText:summary?.textContent.trim(),hasSvg:!!summary?.querySelector('svg'),textLength:text?.length,sameText:paragraph?.textContent.trim()===text,descriptionDisplay:hidden.display,descriptionRect:description?.getBoundingClientRect().toJSON(),paragraphRect:paragraph?.getBoundingClientRect().toJSON(),paragraphVisibility:getComputedStyle(paragraph).visibility,paragraphDisplay:getComputedStyle(paragraph).display,detailsHeight:details?.getBoundingClientRect().height,detailsClientHeight:details?.clientHeight,detailsScrollHeight:details?.scrollHeight,checkVisibility:typeof paragraph?.checkVisibility==='function'?paragraph.checkVisibility():null};
      window.__listKeyboardHelp={target,content,description,details,summary,paragraph,focus:document.activeElement,height:content.clientHeight,left:content.scrollLeft,top:content.scrollTop};
      summary.focus({preventScroll:true});const focused=document.activeElement===summary;summary.click();return {ok:focused,name,description:description.id};
    })()"#
        .replace("TARGET", &json!(target).to_string())
        .replace("CONTENT", &json!(content).to_string())
        .replace("DESCRIPTION", &json!(description).to_string())
        .replace("LABEL", &json!(label).to_string());
    let collapsed = checked(desktop, &setup).await?;
    let expanded = wait_dom(desktop, r#"(()=>{
      const h=window.__listKeyboardHelp,r=h.details.getBoundingClientRect(),p=h.paragraph.getBoundingClientRect(),c=h.content.getBoundingClientRect(),style=getComputedStyle(h.details);
      const bounded=r.left>=0&&r.right<=innerWidth&&r.top>=0&&r.bottom<=innerHeight&&h.details.scrollWidth<=h.details.clientWidth+1;
      const reachable=h.details.scrollHeight<=h.details.clientHeight+1||['auto','scroll'].includes(style.overflowY);
      return {ok:h.details.open&&h.paragraph.checkVisibility()===true&&p.height>0&&bounded&&reachable&&c.height>0&&c.bottom<=innerHeight&&document.activeElement===h.summary,bounded,reachable,helpHeight:r.height,contentHeight:c.height};
    })()"#).await?;
    checked(
        desktop,
        r#"(()=>{const h=window.__listKeyboardHelp;h.summary.click();return {ok:true};})()"#,
    )
    .await?;
    wait_dom(desktop, r#"(()=>{
      const h=window.__listKeyboardHelp;
      return {ok:!h.details.open&&h.paragraph.checkVisibility()===false&&h.target.isConnected&&h.content.isConnected&&Math.abs(h.content.clientHeight-h.height)<=1&&Math.abs(h.content.scrollLeft-h.left)<=1&&Math.abs(h.content.scrollTop-h.top)<=1&&document.activeElement===h.summary,height:h.content.clientHeight,before:h.height};
    })()"#).await?;
    checked(desktop, r#"(()=>{const h=window.__listKeyboardHelp;if(h.focus?.isConnected&&h.focus!==document.body)h.focus.focus({preventScroll:true});const ok=h.focus===document.body||document.activeElement===h.focus;delete window.__listKeyboardHelp;return {ok};})()"#).await?;
    Ok(
        json!({"collapsedAccessible":true,"descriptionPreserved":true,"nativeDisclosure":true,"expandedBounded":true,"closeRestoresContent":true,"focusRestored":true,"collapsed":collapsed,"expanded":expanded}),
    )
}
