//! Native DOM geometry and synthetic navigation, not trusted OS key delivery.
use super::*;

pub(super) async fn run(desktop: &dioxus::desktop::DesktopContext) -> Result<Value, String> {
    verify(desktop, 0).await
}

pub(super) async fn verify(
    desktop: &dioxus::desktop::DesktopContext,
    primary_index: usize,
) -> Result<Value, String> {
    checked(desktop, r#"(()=>{
      const s=document.querySelector('.slack-list-scroll'),row=s.querySelector('tbody tr[data-row-id]');
      s.scrollTop=0;s.scrollLeft=0;row.cells[0].focus({preventScroll:true});
      row.cells[0].dispatchEvent(new KeyboardEvent('keydown',{key:'End',bubbles:true,cancelable:true}));
      return {ok:true};
    })()"#).await?;
    let geometry = wait_dom(desktop, &format!(r#"(()=>{{
      const s=document.querySelector('.slack-list-scroll'),bounds=s.getBoundingClientRect(),row=s.querySelector('tbody tr[data-row-id]');
      const pin=row.querySelector('[data-pinned="true"]'),head=s.querySelector('th[data-pinned="true"]'),last=row.cells[row.cells.length-1];
      if(!pin||!head)return {{ok:false,pin:!!pin,head:!!head}};
      const p=pin.getBoundingClientRect(),h=head.getBoundingClientRect(),r=last.getBoundingClientRect();
      const hit=(element,rect)=>element.contains(document.elementFromPoint(rect.left+Math.min(12,rect.width/2),rect.top+Math.min(12,rect.height/2)));
      const pinned=pin.cellIndex==={primary_index}&&head.dataset.columnId===pin.dataset.columnId&&Math.abs(p.left-bounds.left)<2&&Math.abs(h.left-p.left)<2;
      const revealed=r.left>=p.right-1&&r.right<=bounds.right+1&&r.top>=h.bottom-1&&document.activeElement===last;
      const check=row.querySelector('[data-column-size="checkbox"]'),number=row.querySelector('[data-column-size="number"]'),standard=row.querySelector('[data-column-size="standard"]:not([data-pinned="true"])');
      const widths=[check?.getBoundingClientRect().width,number?.getBoundingClientRect().width,standard?.getBoundingClientRect().width];
      const compact=widths.every(Number.isFinite)&&widths[0]<widths[2]&&widths[1]<widths[2];
      const opaque=getComputedStyle(pin).backgroundColor===getComputedStyle(last).backgroundColor&&getComputedStyle(pin).backgroundColor!=='rgba(0, 0, 0, 0)';
      const clearance=parseFloat(s.style.getPropertyValue('--list-pinned-width'))>=p.width-1;
      return {{ok:pinned&&revealed&&compact&&opaque&&clearance&&hit(pin,p)&&hit(head,h)&&hit(last,r)&&s.scrollLeft>0,pinned,revealed,compact,opaque,clearance,widths,scroll:s.scrollLeft,pin:[p.left,p.right],last:[r.left,r.right],bounds:[bounds.left,bounds.right]}};
    }})()"#)).await?;
    checked(desktop, r#"(()=>{
      document.activeElement.dispatchEvent(new KeyboardEvent('keydown',{key:'Home',bubbles:true,cancelable:true}));
      return {ok:true};
    })()"#).await?;
    wait_dom(desktop,r#"(()=>{
      const s=document.querySelector('.slack-list-scroll'),first=s.querySelector('tbody tr[data-row-id]').cells[0],r=first.getBoundingClientRect();
      return {ok:document.activeElement===first&&s.scrollLeft===0&&first.contains(document.elementFromPoint(r.left+12,r.top+12))};
    })()"#).await?;
    checked(desktop,r#"(()=>{
      document.activeElement.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowRight',bubbles:true,cancelable:true}));return {ok:true};
    })()"#).await?;
    wait_dom(desktop,r#"(()=>{
      const s=document.querySelector('.slack-list-scroll'),cell=s.querySelector('tbody tr[data-row-id]').cells[1],r=cell.getBoundingClientRect();
      return {ok:document.activeElement===cell&&cell.contains(document.elementFromPoint(r.left+12,r.top+12))&&document.querySelectorAll('.slack-list-grid td[tabindex="0"]').length===1};
    })()"#).await?;
    checked(desktop,r#"(()=>{const s=document.querySelector('.slack-list-scroll');s.scrollTop=0;s.scrollLeft=0;s.querySelector('tbody td').focus({preventScroll:true});return {ok:true};})()"#).await?;
    Ok(
        json!({"compactColumns":true,"primaryPinned":true,"opaqueFocusState":true,"syntheticNavigationHitTest":true,"homeRestoresLeft":true,"geometry":geometry}),
    )
}

pub(super) async fn hidden(desktop: &dioxus::desktop::DesktopContext) -> Result<(), String> {
    wait_dom(desktop,r#"(()=>{
      const s=document.querySelector('.slack-list-scroll');
      return {ok:!s.querySelector('[data-pinned="true"]')&&parseFloat(s.style.getPropertyValue('--list-pinned-width'))===0};
    })()"#).await?;
    Ok(())
}
