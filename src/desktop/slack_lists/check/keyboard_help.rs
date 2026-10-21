//! Native details disclosure checks; no synthetic keyboard default actions.
use super::*;

pub(super) async fn run(
    desktop: &dioxus::desktop::DesktopContext,
    surface: &str,
) -> Result<Value, String> {
    let (target, content, description, disclosure, label) = match surface {
        "table" => (
            ".slack-list-grid",
            ".slack-list-scroll",
            "slack-list-keyboard",
            ":scope > #slack-list-shortcuts",
            "List keyboard shortcuts",
        ),
        "board" => (
            ".slack-list-board",
            ".slack-list-board",
            "slack-list-board-help",
            ":scope > details.slack-list-board-shortcuts",
            "Keyboard shortcuts",
        ),
        "record" => (
            "#slack-list-record-detail",
            ".slack-list-detail-scroll",
            "slack-list-record-help",
            ":scope > #slack-list-record-shortcuts",
            "Item keyboard shortcuts",
        ),
        _ => return Err("Unknown keyboard help surface".into()),
    };
    let setup = r#"(()=>{
      const target=document.querySelector(TARGET),content=document.querySelector(CONTENT),description=document.getElementById(DESCRIPTION);
      const scope=target?.matches('#slack-list-record-detail')?target:target?.closest('.slack-list-layout');
      const disclosures=[...(scope?.querySelectorAll(DISCLOSURE)||[])],details=disclosures.length===1?disclosures[0]:null;
      const summary=details?.querySelector(':scope > summary'),paragraph=details?.querySelector(':scope > p');
      if(!target||!content||!description||!scope?.contains(description)||!details||!summary||!paragraph)
        return {ok:false,reason:'Original surface disclosure missing or ambiguous',disclosures:disclosures.length,target:!!target,content:!!content,description:!!description,summary:!!summary,paragraph:!!paragraph};
      const offsets=[];
      for(let e=details;e;e=e.parentElement){
        offsets.push({node:e,left:e.scrollLeft,top:e.scrollTop});
        if(e.matches('.slack-list-dialog'))break;
      }
      const proof=e=>{
        const r=e.getBoundingClientRect(),finite=[r.x,r.y,r.width,r.height].every(Number.isFinite);
        let contained=true;
        for(let p=e.parentElement;p;p=p.parentElement){
          const s=getComputedStyle(p),q=p.getBoundingClientRect(),left=q.left+p.clientLeft,top=q.top+p.clientTop;
          if(['auto','scroll','hidden','clip'].includes(s.overflowX))contained&&=r.left>=left-1&&r.right<=left+p.clientWidth+1;
          if(['auto','scroll','hidden','clip'].includes(s.overflowY))contained&&=r.top>=top-1&&r.bottom<=top+p.clientHeight+1;
        }
        const hit=document.elementFromPoint(r.left+r.width/2,r.top+r.height/2);
        return {rect:r.toJSON(),bounded:finite&&r.width>0&&r.height>0&&r.left>=0&&r.right<=innerWidth+1&&r.top>=0&&r.bottom<=innerHeight+1&&contained,hit:!!hit&&(hit===e||e.contains(hit))};
      };
      const focus=document.activeElement,height=content.clientHeight,left=content.scrollLeft,top=content.scrollTop;
      summary.scrollIntoView({block:'nearest',inline:'nearest',behavior:'instant'});
      const name=summary.getAttribute('aria-label')||summary.textContent.trim(),paint=proof(summary),rect=summary.getBoundingClientRect();
      const described=target?.getAttribute('aria-describedby')?.split(/\s+/).includes(DESCRIPTION);
      const hidden=getComputedStyle(description),text=description?.textContent.trim();
      const ok=details.tagName==='DETAILS'&&!details.open&&summary.tabIndex===0&&name===LABEL&&summary.textContent.trim()==='Keyboard shortcuts'&&!!summary.querySelector('svg')&&described&&text.length>80&&paragraph.textContent.trim()===text&&hidden.display!=='none'&&description.getBoundingClientRect().height<=1&&rect.height>=24&&paint.bounded&&paint.hit&&paragraph.checkVisibility()===false&&content.clientHeight>0;
      if(!ok)return {ok:false,name,described,tag:details.tagName,open:details.open,paint,contentHeight:content.clientHeight,tabIndex:summary.tabIndex,summaryText:summary.textContent.trim(),hasSvg:!!summary.querySelector('svg'),textLength:text.length,sameText:paragraph.textContent.trim()===text,descriptionDisplay:hidden.display,descriptionRect:description.getBoundingClientRect().toJSON(),paragraphRect:paragraph.getBoundingClientRect().toJSON(),detailsHeight:details.getBoundingClientRect().height,detailsClientHeight:details.clientHeight,detailsScrollHeight:details.scrollHeight,checkVisibility:paragraph.checkVisibility()};
      window.__listKeyboardHelp={target,content,description,scope,details,summary,paragraph,focus,height,left,top,offsets,proof};
      summary.focus({preventScroll:true});const focused=document.activeElement===summary;summary.click();return {ok:focused,name,description:description.id,disclosure:DISCLOSURE,paint};
    })()"#
        .replace("TARGET", &json!(target).to_string())
        .replace("CONTENT", &json!(content).to_string())
        .replace("DESCRIPTION", &json!(description).to_string())
        .replace("DISCLOSURE", &json!(disclosure).to_string())
        .replace("LABEL", &json!(label).to_string());
    let collapsed = checked(desktop, &setup).await?;
    let expanded = wait_dom(desktop, r#"(()=>{
      const h=window.__listKeyboardHelp;
      h.details.scrollIntoView({block:'nearest',inline:'nearest',behavior:'instant'});
      // A short record surface intentionally scrolls the disclosure itself.
      // Prove the original control and full text at their own scroll positions.
      h.summary.scrollIntoView({block:'nearest',inline:'nearest',behavior:'instant'});
      const summary=h.proof(h.summary),disclosure=h.proof(h.details),summaryScroll={left:h.details.scrollLeft,top:h.details.scrollTop};
      h.paragraph.scrollIntoView({block:'nearest',inline:'nearest',behavior:'instant'});
      const paragraph=h.proof(h.paragraph),paragraphDisclosure=h.proof(h.details),paragraphScroll={left:h.details.scrollLeft,top:h.details.scrollTop};
      const r=h.details.getBoundingClientRect(),p=h.paragraph.getBoundingClientRect(),c=h.content.getBoundingClientRect(),style=getComputedStyle(h.details);
      const bounded=disclosure.bounded&&summary.bounded&&summary.hit&&paragraphDisclosure.bounded&&paragraph.bounded&&paragraph.hit&&h.details.scrollWidth<=h.details.clientWidth+1;
      const reachable=h.details.scrollHeight<=h.details.clientHeight+1||['auto','scroll'].includes(style.overflowY);
      const original=h.target.isConnected&&h.details.contains(h.summary)&&h.details.contains(h.paragraph)&&h.paragraph.textContent.trim()===h.description.textContent.trim();
      return {ok:original&&h.details.open&&h.paragraph.checkVisibility()===true&&p.height>0&&bounded&&reachable&&c.height>0&&c.bottom<=innerHeight+1&&document.activeElement===h.summary,bounded,reachable,original,summary,paragraph,disclosure,paragraphDisclosure,summaryScroll,paragraphScroll,helpHeight:r.height,contentHeight:c.height};
    })()"#).await?;
    checked(
        desktop,
        r#"(()=>{const h=window.__listKeyboardHelp;h.summary.scrollIntoView({block:'nearest',inline:'nearest',behavior:'instant'});const paint=h.proof(h.summary);if(!paint.bounded||!paint.hit)return {ok:false,paint};h.summary.click();for(const s of h.offsets){s.node.scrollLeft=s.left;s.node.scrollTop=s.top;}return {ok:true,paint};})()"#,
    )
    .await?;
    wait_dom(desktop, r#"(()=>{
      const h=window.__listKeyboardHelp;
      const offsets=h.offsets.map(s=>({connected:s.node.isConnected,left:s.node.scrollLeft,top:s.node.scrollTop,expectedLeft:s.left,expectedTop:s.top}));
      return {ok:!h.details.open&&h.paragraph.checkVisibility()===false&&h.target.isConnected&&h.content.isConnected&&Math.abs(h.content.clientHeight-h.height)<=1&&Math.abs(h.content.scrollLeft-h.left)<=1&&Math.abs(h.content.scrollTop-h.top)<=1&&offsets.every(s=>s.connected&&Math.abs(s.left-s.expectedLeft)<=1&&Math.abs(s.top-s.expectedTop)<=1)&&document.activeElement===h.summary,height:h.content.clientHeight,before:h.height,offsets};
    })()"#).await?;
    checked(desktop, r#"(()=>{const h=window.__listKeyboardHelp;if(h.focus?.isConnected&&h.focus!==document.body)h.focus.focus({preventScroll:true});const ok=h.focus===document.body||document.activeElement===h.focus;delete window.__listKeyboardHelp;return {ok};})()"#).await?;
    Ok(
        json!({"collapsedAccessible":true,"descriptionPreserved":true,"nativeDisclosure":true,"expandedBounded":true,"closeRestoresContent":true,"focusRestored":true,"collapsed":collapsed,"expanded":expanded}),
    )
}
