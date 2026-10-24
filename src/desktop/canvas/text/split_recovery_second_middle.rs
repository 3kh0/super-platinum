//! Individually reveal bounded semantic parts of the retained two-split review.
use super::*;

pub(super) const STAGES: [(&str, &str, &str, Option<usize>); 9] = [
    (
        "review-original",
        "original",
        "[data-review-split=original]",
        None,
    ),
    (
        "review-first-boundary",
        "first-boundary",
        "[data-review-first-halves]",
        None,
    ),
    (
        "review-second-history",
        "history",
        "[data-second-history]",
        None,
    ),
    (
        "review-second-before",
        "before",
        "[data-second-before]",
        None,
    ),
    ("review-second-left", "left", "[data-second-left]", None),
    ("review-second-right", "right", "[data-second-right]", None),
    ("review-member-0", "member", "[data-second-member]", Some(0)),
    ("review-member-1", "member", "[data-second-member]", Some(1)),
    ("review-member-2", "member", "[data-second-member]", Some(2)),
];

const READY: &str = r#"(()=>{
    const p=document.querySelector('.canvas-local-review'),c=p?.querySelector('input'),n=p?.querySelector('[data-review-second-middle]');
    const expected=EXPECTED;
    const texts=s=>[...(p?.querySelectorAll(s)||[])].map(n=>n.textContent);
    const members=[...(p?.querySelectorAll('[data-second-member]')||[])];
    const same=(a,b)=>JSON.stringify(a)===JSON.stringify(b);
    return {ok:!!c&&!c.checked&&!c.disabled&&n?.dataset.target===expected.target
        &&same(texts('[data-review-split=original] .canvas-review-preview'),[expected.original])
        &&same(texts('[data-review-first-halves] .canvas-review-preview'),expected.first)
        &&same(texts('[data-second-before] .canvas-review-preview'),[expected.before])
        &&same(texts('[data-second-left] .canvas-review-preview'),[expected.left])
        &&same(texts('[data-second-right] .canvas-review-preview'),[expected.right])
        &&same(texts('[data-second-history]'),[expected.historyNotice])
        &&same(members.map(n=>n.dataset.secondMember),expected.visualOrder)
        &&members.every(n=>{const index=Number(n.dataset.creationIndex),e=expected.creation[index];return Number.isInteger(index)&&n.dataset.creationIndex===String(index)&&!!e&&e.kind===n.dataset.secondMember
            &&n.querySelector('.canvas-review-preview')?.textContent===e.text
            &&(n.querySelector('[data-member-empty]')?.textContent??null)===e.emptyLabel;})};
})()"#;

pub(super) fn ready() -> String {
    let mut expected = super::super::split_fixture::second_middle::review();
    let corpus = super::super::split_fixture::corpus();
    expected["original"] = json!(corpus.seed);
    expected["first"] = json!([corpus.left, corpus.right]);
    READY.replace("EXPECTED", &expected.to_string())
}

const TARGET: &str = r#"const panel=document.querySelector('.canvas-local-review');
const nodes=[...(panel?.querySelectorAll(SELECTOR)||[])],visualIndex=INDEX;
const n=visualIndex===null?(nodes.length===1?nodes[0]:null):(nodes.length===3?nodes[visualIndex]:null);"#;

const GEOMETRY: &str = r#"(()=>{
TARGET
const s=n?.closest('.canvas-scroll'),r=n?.getBoundingClientRect(),b=s?.getBoundingClientRect();
const visible=!!n?.isConnected&&!!r&&!!b&&r.width>0&&r.height>0&&b.width>0&&b.height>0
    &&r.left>=Math.max(0,b.left)-1&&r.right<=Math.min(innerWidth,b.right)+1
    &&r.top>=Math.max(0,b.top)-1&&r.bottom<=Math.min(innerHeight,b.bottom)+1;
return {ok:visible,secondMiddle:true,section:SECTION,visualIndex,
    creationIndex:visualIndex===null?null:Number(n?.dataset.creationIndex),memberKind:visualIndex===null?null:n?.dataset.secondMember??null,
    visible,rect:r?[r.x,r.y,r.width,r.height]:null,clip:b?[b.x,b.y,b.width,b.height]:null,viewport:[innerWidth,innerHeight],
    text:n?.textContent??null,previews:[...(n?.querySelectorAll('.canvas-review-preview')||[])].map(n=>n.textContent),
    emptyLabel:n?.querySelector('[data-member-empty]')?.textContent??null};
})()"#;

pub(super) async fn reveal(
    desktop: &dioxus::desktop::DesktopContext,
    index: usize,
) -> Result<Value, String> {
    let (_, section, selector, visual_index) = STAGES
        .get(index)
        .ok_or("Unknown second-middle review stage")?;
    wait(desktop, &ready()).await?;
    let target = TARGET
        .replace("SELECTOR", &json!(selector).to_string())
        .replace("INDEX", &json!(visual_index).to_string());
    let scroll = format!(
        "(()=>{{{target}if(!n)return {{ok:false}};n.scrollIntoView({{block:'center',behavior:'instant'}});return {{ok:true}};}})()"
    );
    checked(desktop, &scroll).await?;
    wait(desktop, &ready()).await?;
    let geometry = GEOMETRY
        .replace("TARGET", &target)
        .replace("SECTION", &json!(section).to_string());
    wait(desktop, &geometry).await
}
