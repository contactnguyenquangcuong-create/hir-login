//! The Facebook tab's steps: check the account, watch a video, react, comment,
//! share to a group or the profile, save.
//!
//! Facebook's class names are generated and change; aria-labels and the text a
//! person reads are what stays, so each step finds its element by those (Vietnamese
//! and English), tags it `data-hir=…` from the isolated world, and then does a
//! normal trusted click on the tag — the same approach as the shipped
//! "chia sẻ nhóm" project, whose finders are reused here.
//!
//! Every step that posts something public takes `dry`: with `dry` = 1 it goes
//! as far as the button and stops without pressing it, so the first run can be
//! checked against the real page.

use super::{center_waiting, click_at, expand, param, param_f64, touch_front, viewport_center, Flow, Run};
use crate::cdp;
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration;

// ---- page-side finders -------------------------------------------------------

const PRE: &str = r##"var vis=function(e){var r=e.getBoundingClientRect();if(r.width<3||r.height<3)return false;var s=getComputedStyle(e);return s.visibility!=='hidden'&&s.display!=='none';};var txt=function(e){return (e.innerText||'').replace(/\s+/g,' ').trim();};var lab=function(e){return (e.getAttribute('aria-label')||'').trim();};var topDlg=function(){var d=[].slice.call(document.querySelectorAll('div[role="dialog"]')).filter(vis);var a=function(e){var r=e.getBoundingClientRect();return r.width*r.height;};d.sort(function(x,y){return a(y)-a(x);});return d.length?d[0]:document.body;};var clear=function(){[].slice.call(document.querySelectorAll('[data-hir]')).forEach(function(e){e.removeAttribute('data-hir');});};var tag=function(e,n){e.setAttribute('data-hir',n);e.scrollIntoView({block:'center'});return 'HIR_OK';};"##;

fn js(body: &str) -> String {
    format!("(function(){{{PRE}{body}}})()")
}

/// Checkpoint / "slow down" / identity pages. Never pushed through.
fn js_guard() -> String {
    js(r##"var t=((document.body&&document.body.innerText)||'').slice(0,30000);var bad=/(tạm thời bị chặn|temporarily blocked|bạn tạm thời không thể|hãy chậm lại|xác nhận danh tính|confirm your identity|xác thực hai yếu tố|two-factor|tài khoản của bạn đã bị hạn chế|your account has been restricted)/i;return (bad.test(t)||/checkpoint|two_step/.test(location.href))?'HIR_BLOCKED':'HIR_OK';"##)
}

fn js_logged_out() -> String {
    js(r##"var p=document.querySelector('input[name="pass"]');return ((p&&vis(p))||/\/login/.test(location.pathname))?'HIR_LOGGED_OUT':'HIR_OK';"##)
}

/// Starts the video if the page left it paused (autoplay is often held back).
fn js_play() -> String {
    js(r##"var v=[].slice.call(document.querySelectorAll('video')).filter(vis)[0];if(!v)return 'HIR_NOVIDEO';if(v.paused){var p=v.play();if(p&&p.catch)p.catch(function(){});return 'HIR_STARTED';}return 'HIR_PLAYING';"##)
}

/// The post's own Like button. HIR_DONE when it already shows a reaction
/// (the label then starts with "Gỡ"/"Remove"), so a second run never un-likes.
fn js_like() -> String {
    js(r##"clear();var root=topDlg();var done=/^(gỡ|remove|bỏ|unlike)/i,like=/^(thích|like)$/i;var all=[].slice.call(root.querySelectorAll('[role="button"][aria-label]')).filter(vis);for(var i=0;i<all.length;i++){var l=lab(all[i]);if(done.test(l))return 'HIR_DONE:'+l;if(like.test(l))return tag(all[i],'like');}return 'HIR_NONE';"##)
}

/// The post's reaction button whatever it currently says ("Thích", or "Gỡ Thích" /
/// "Gỡ Yêu thích" once a reaction is set): the thing to rest the pointer on so the
/// reaction bar opens.
fn js_react_anchor() -> String {
    js(r##"clear();var root=topDlg();var re=/^(thích|like)$|^(gỡ|remove)\s/i;var c=[].slice.call(root.querySelectorAll('[role="button"][aria-label]')).filter(function(e){return vis(e)&&re.test(lab(e));});if(!c.length)return 'HIR_NONE';return tag(c[0],'like');"##)
}

/// Words a reaction goes by on its "remove" label, Vietnamese and English.
fn reaction_words(which: &str) -> &'static [&'static str] {
    match which {
        "love" => &["yêu thích", "love"],
        "care" => &["thương thương", "care"],
        "haha" => &["haha"],
        "wow" => &["wow"],
        "sad" => &["buồn", "sad"],
        "angry" => &["phẫn nộ", "angry"],
        _ => &["thích", "like"],
    }
}

/// One of the reactions in the bar that opens when the pointer rests on Like.
fn js_reaction(which: &str) -> String {
    let re = match which {
        "love" => "^(yêu thích|love)$",
        "care" => "^(thương thương|care)$",
        "haha" => "^haha$",
        "wow" => "^wow$",
        "sad" => "^(buồn|sad)$",
        "angry" => "^(phẫn nộ|angry)$",
        _ => "^(thích|like)$",
    };
    js(&format!(
        r##"clear();var re=/{re}/i;var c=[].slice.call(document.querySelectorAll('[aria-label]')).filter(function(e){{return vis(e)&&re.test(lab(e));}});var v=c.filter(function(e){{var r=e.getBoundingClientRect();return r.top>=0&&r.bottom<=innerHeight;}});if(v.length)c=v;if(!c.length)return 'HIR_NONE';c.sort(function(a,b){{return ((b.getAttribute('role')==='button')?1:0)-((a.getAttribute('role')==='button')?1:0);}});return tag(c[0],'react');"##
    ))
}

fn js_comment_box() -> String {
    js(r##"clear();var root=topDlg();var c=[].slice.call(root.querySelectorAll('[contenteditable="true"][role="textbox"]')).filter(function(e){return vis(e)&&/(bình luận|comment)/i.test(lab(e));});if(!c.length)return 'HIR_NONE';return tag(c[0],'cbox');"##)
}

/// The "Bình luận" button of a reel / video: the comment box only exists after it is pressed.
fn js_comment_button() -> String {
    js(r##"clear();var c=[].slice.call(document.querySelectorAll('[role="button"][aria-label]')).filter(function(e){return vis(e)&&/^(bình luận|comment)$/i.test(lab(e));});if(!c.length)return 'HIR_NONE';return tag(c[0],'cbtn');"##)
}

/// A "Lưu" button on the post itself (reels and videos have one beside Share);
/// HIR_DONE when it already reads "Bỏ lưu" / "Đã lưu".
fn js_save_button() -> String {
    js(r##"clear();var all=[].slice.call(document.querySelectorAll('[role="button"][aria-label]')).filter(vis);for(var i=0;i<all.length;i++){var l=lab(all[i]);if(/^(bỏ lưu|đã lưu|unsave|saved)$/i.test(l))return 'HIR_DONE';if(/^(lưu|save)$/i.test(l))return tag(all[i],'savebtn');}return 'HIR_NONE';"##)
}

/// The "Bày tỏ cảm xúc" / "React" button beside Like: pressing it opens the reaction bar.
fn js_react_button() -> String {
    js(r##"clear();var inView=function(e){var r=e.getBoundingClientRect();return r.top>=0&&r.bottom<=innerHeight;};var c=[].slice.call(document.querySelectorAll('[role="button"][aria-label]')).filter(function(e){return vis(e)&&/^(bày tỏ cảm xúc|react|thay đổi cảm xúc|change reaction)/i.test(lab(e));});var v=c.filter(inView);if(v.length)c=v;if(!c.length)return 'HIR_NONE';return tag(c[0],'rbtn');"##)
}

/// Presses the Comment button from the page side. A fallback for when the pointer's
/// own click did not open the box.
fn js_comment_press() -> String {
    js(r##"var c=[].slice.call(document.querySelectorAll('[role="button"][aria-label]')).filter(function(e){return vis(e)&&/^(bình luận|comment)$/i.test(lab(e));});if(!c.length)return 'HIR_NONE';c[0].scrollIntoView({block:'center'});c[0].click();return 'HIR_OK';"##)
}

fn js_share_button() -> String {
    js(r##"clear();var re=/(gửi nội dung này cho bạn bè|send this to friends|^chia sẻ$|^share$)/i;var c=[].slice.call(document.querySelectorAll('[role="button"]')).filter(function(e){return vis(e)&&!e.closest('div[role="dialog"]')&&(re.test(lab(e))||re.test(txt(e)));});if(!c.length)return 'HIR_NONE';return tag(c[0],'share');"##)
}

/// Presses the Share button from the page side — the fallback when the pointer's own click
/// did not open the dialog.
fn js_share_press() -> String {
    js(r##"var re=/(gửi nội dung này cho bạn bè|send this to friends|^chia sẻ$|^share$)/i;var c=[].slice.call(document.querySelectorAll('[role="button"]')).filter(function(e){return vis(e)&&!e.closest('div[role="dialog"]')&&(re.test(lab(e))||re.test(txt(e)));});if(!c.length)return 'HIR_NONE';c[0].scrollIntoView({block:'center'});c[0].click();return 'HIR_OK';"##)
}

/// How many dialogs are open on the page, as text.
async fn dialog_count(profile: &str) -> String {
    script(profile, "String(document.querySelectorAll('div[role=\"dialog\"]').length)")
        .await
        .map(|n| n.trim_matches('"').to_string())
        .unwrap_or_default()
}

/// What the open dialog offers, for an error message: how many dialogs, and the short
/// clickable texts in them.
fn js_menu_diag() -> String {
    js(r##"var d=[].slice.call(document.querySelectorAll('div[role="dialog"]')).filter(vis);var items=[].slice.call(topDlg().querySelectorAll('[role="button"],[role="menuitem"],[role="link"],[role="radio"],[role="option"]')).filter(function(e){var t=txt(e);return vis(e)&&t.length>1&&t.length<50;}).map(txt);var seen={};items=items.filter(function(t){if(seen[t])return false;seen[t]=1;return true;});return JSON.stringify({dialogs:d.length,items:items.slice(0,14)});"##)
}

/// "Chia sẻ lên nhóm" / "Share to a group" in the menu the Share button opens.
fn js_group_item() -> String {
    js(r##"clear();var root=topDlg();var re=/^(chia sẻ (lên|vào) nhóm|nhóm$|share to a group|share in a group|groups?$)/i;var c=[].slice.call(root.querySelectorAll('[role="button"],[role="menuitem"],[role="link"],[tabindex]')).filter(function(e){return vis(e)&&re.test(txt(e));});if(!c.length)return 'HIR_NONE';c.sort(function(a,b){return txt(a).length-txt(b).length;});return tag(c[0],'menu');"##)
}

/// "Share now" or "Share to Feed" in the same menu. `caption` decides which: a
/// caption needs the dialog, so the feed item; without one, "Share now" is the
/// one-click share.
fn js_profile_item(with_caption: bool) -> String {
    let re = if with_caption {
        "^(chia sẻ lên (bảng feed|dòng thời gian|trang cá nhân)|share to (your )?(feed|profile|timeline))"
    } else {
        "^(chia sẻ ngay|share now)"
    };
    js(&format!(
        r##"clear();var root=topDlg();var re=/{re}/i;var c=[].slice.call(root.querySelectorAll('[role="button"],[role="menuitem"],[role="link"],[tabindex]')).filter(function(e){{return vis(e)&&re.test(txt(e));}});if(!c.length)return 'HIR_NONE';c.sort(function(a,b){{return txt(a).length-txt(b).length;}});return tag(c[0],'menu');"##
    ))
}

fn js_scroll_picker() -> String {
    js(r##"var root=topDlg();var rows=[].slice.call(root.querySelectorAll('[role="button"],[role="radio"],[role="option"],[role="listitem"]')).filter(vis);if(!rows.length)return 'HIR_NONE';var p=rows[0].parentElement;while(p&&p!==document.body){if(p.scrollHeight>p.clientHeight+30){var o=getComputedStyle(p).overflowY;if(o==='auto'||o==='scroll')break;}p=p.parentElement;}if(p&&p!==document.body){p.scrollTop=Math.floor(Math.random()*Math.max(1,p.scrollHeight-p.clientHeight)*0.85);return 'HIR_SCROLLED';}return 'HIR_FLAT';"##)
}

/// A group from the picker: the one named, else a random one not shared to yet.
fn js_pick_group(name: &str, done: &str) -> String {
    let q = |s: &str| s.replace('\\', "\\\\").replace('\'', "\\'").replace('\n', " ");
    js(&format!(
        r##"clear();var want=('{}').toLowerCase();var done=('{}').split('||');var root=topDlg();var skip=/^(đăng|post|hủy|huỷ|cancel|đóng|close|quay lại|back|tìm kiếm|search|xong|done|chia sẻ lên nhóm|share to a group|chia sẻ|share)$/i;var seen={{}};var c=[];[].slice.call(root.querySelectorAll('[role="button"],[role="radio"],[role="option"],[role="listitem"]')).forEach(function(e){{if(!vis(e))return;var name=((e.innerText||'').split('\n')[0]||'').replace(/['"\\]/g,'').trim();if(name.length<2||name.length>80||skip.test(name)||seen[name])return;if(want){{if(name.toLowerCase().indexOf(want)<0)return;}}else if(done.indexOf(name)>=0)return;seen[name]=1;c.push([e,name]);}});if(!c.length)return 'HIR_NONE';var k=want?c[0]:c[Math.floor(Math.random()*c.length)];document.documentElement.setAttribute('data-hir-name',k[1]);return tag(k[0],'pick');"##,
        q(&name.to_lowercase()),
        q(done)
    ))
}

/// The search box of the group picker, to type a group's name into.
fn js_search_box() -> String {
    js(r##"clear();var root=topDlg();var c=[].slice.call(root.querySelectorAll('input[type="search"],input[type="text"],input:not([type])')).filter(function(e){return vis(e)&&/(tìm kiếm|search)/i.test((e.getAttribute('aria-label')||'')+' '+(e.getAttribute('placeholder')||''));});if(!c.length)return 'HIR_NONE';return tag(c[0],'gsearch');"##)
}

/// What the page says is going on when the comment box cannot be found.
fn js_comment_diag() -> String {
    js(r##"var ed=[].slice.call(document.querySelectorAll('[contenteditable],[role="textbox"],textarea')).filter(vis).map(function(e){var r=e.getBoundingClientRect();return e.tagName.toLowerCase()+'['+(e.getAttribute('contenteditable')||'')+'|'+(e.getAttribute('role')||'')+'] '+lab(e).slice(0,40)+' @'+Math.round(r.x)+','+Math.round(r.y);});var cb=[].slice.call(document.querySelectorAll('[role="button"][aria-label]')).filter(function(e){return /^(bình luận|comment)$/i.test(lab(e));}).map(function(e){var r=e.getBoundingClientRect();return Math.round(r.x)+','+Math.round(r.y)+' '+Math.round(r.width)+'x'+Math.round(r.height);});return JSON.stringify({url:location.pathname,scrollY:Math.round(scrollY),innerH:innerHeight,dialogs:document.querySelectorAll('div[role="dialog"]').length,editors:ed.slice(0,4),commentButtons:cb.slice(0,3)});"##)
}

/// Whether the dialog is the group list and not the "send to friends on Messenger"
/// one. Both live in the same Share dialog; picking a row in the second one is picking a
/// person, so nothing is picked until this says HIR_OK.
fn js_group_picker_open() -> String {
    js(r##"var t=(topDlg().innerText||'').replace(/\s+/g,' ');if(/(gửi bằng messenger|send in messenger|gửi cho bạn bè)/i.test(t))return 'HIR_FRIENDS';if(!/(nhóm|group)/i.test(t))return 'HIR_UNKNOWN';return 'HIR_OK';"##)
}

fn js_caption_box() -> String {
    js(r##"clear();var root=topDlg();var c=[].slice.call(root.querySelectorAll('[contenteditable="true"][role="textbox"]')).filter(vis);if(!c.length)return 'HIR_NONE';return tag(c[0],'caption');"##)
}

fn js_post_button() -> String {
    js(r##"clear();var root=topDlg();var re=/^(đăng|post|chia sẻ|share|chia sẻ ngay|share now)$/i;var c=[].slice.call(root.querySelectorAll('[role="button"]')).filter(function(e){return vis(e)&&(re.test(lab(e))||re.test(txt(e)));});if(!c.length)return 'HIR_NONE';var b=c[c.length-1];if(b.getAttribute('aria-disabled')==='true')return 'HIR_DISABLED';return tag(b,'post');"##)
}

fn js_more_button() -> String {
    js(r##"clear();var root=topDlg();var re=/(hành động với bài viết này|actions for this post|tùy chọn khác|more options|xem thêm tùy chọn)/i;var c=[].slice.call(root.querySelectorAll('[role="button"][aria-label]')).filter(function(e){return vis(e)&&re.test(lab(e));});if(!c.length)return 'HIR_NONE';return tag(c[0],'more');"##)
}

/// "Lưu video / bài viết / reel" in the post's menu; HIR_DONE when it already
/// reads "Bỏ lưu" (saved).
fn js_save_item() -> String {
    js(r##"clear();var items=[].slice.call(document.querySelectorAll('[role="menuitem"],[role="button"]')).filter(vis);for(var i=0;i<items.length;i++){var t=txt(items[i]);if(/^(bỏ lưu|unsave)/i.test(t))return 'HIR_DONE';}var c=items.filter(function(e){return /^(lưu (video|bài viết|reel)|save (video|post|reel))/i.test(txt(e));});if(!c.length)return 'HIR_NONE';c.sort(function(a,b){return txt(a).length-txt(b).length;});return tag(c[0],'save');"##)
}

// ---- helpers -----------------------------------------------------------------

async fn script(profile: &str, src: &str) -> Result<String> {
    let out = cdp::page_call(profile, "Script.run", json!({ "source": src })).await?;
    Ok(out.get("result").and_then(|v| v.as_str()).unwrap_or("null").to_string())
}

async fn tap(mobile: bool, profile: &str, x: f64, y: f64) -> Result<()> {
    if mobile {
        touch_front(profile).await;
        cdp::page_call(profile, "Motion.touchTap", json!({ "x": x, "y": y })).await?;
        Ok(())
    } else {
        click_at(profile, x, y, "left").await
    }
}

/// Tags with `finder`, retrying while the page builds, then clicks the tag.
/// Returns the finder's last answer so a caller can tell HIR_DONE from a click.
async fn find_and_click(
    mobile: bool,
    profile: &str,
    finder: &str,
    tag_name: &str,
    what: &str,
    tries: u32,
) -> Result<String> {
    let mut last = String::new();
    for i in 0..tries.max(1) {
        last = script(profile, finder).await?;
        if last.contains("HIR_OK") {
            let (x, y) = center_waiting(profile, &format!("[data-hir=\"{tag_name}\"]"), 8.0).await?;
            tap(mobile, profile, x, y).await?;
            return Ok(last);
        }
        if last.contains("HIR_DONE") {
            return Ok(last);
        }
        if i + 1 < tries {
            tokio::time::sleep(Duration::from_millis(1200)).await;
        }
    }
    Err(anyhow!(
        "Facebook: could not find {what} ({last}) — wrong link, nothing to act on here, or Facebook's page changed"
    ))
}

fn num(p: &Value, name: &str, vars: &HashMap<String, String>) -> Option<f64> {
    param(p, name)
        .map(|v| expand(v, vars))
        .and_then(|v| v.trim().parse::<f64>().ok())
        .or_else(|| param_f64(p, name))
}

fn unit() -> f64 {
    let b = uuid::Uuid::new_v4();
    let n = u16::from_le_bytes([b.as_bytes()[0], b.as_bytes()[1]]);
    f64::from(n) / 65535.0
}

async fn pause(ms: u64) {
    tokio::time::sleep(Duration::from_millis(ms)).await;
}

/// Moves the pointer off to empty page: after a Like or a hover the reaction bar
/// stays open under the pointer and sits over the buttons next to it.
async fn park(profile: &str) {
    let (cx, cy) = viewport_center(profile).await;
    let _ = cdp::page_call(profile, "Motion.createPointer", json!({ "x": cx * 0.55, "y": cy })).await;
    let _ = cdp::page_call(profile, "Motion.glideTo", json!({ "x": cx * 0.55, "y": cy })).await;
    pause(700).await;
}

async fn escape(profile: &str) {
    let _ = cdp::page_call(profile, "Motion.pressKey", json!({ "key": "Escape" })).await;
}

async fn guard(profile: &str) -> Result<()> {
    if script(profile, &js_guard()).await?.contains("HIR_BLOCKED") {
        return Err(anyhow!(
            "Facebook is blocking this account or asking to verify (checkpoint). Stopped, not pushing through — open the profile and deal with it by hand, then run again."
        ));
    }
    Ok(())
}

/// `dry` is a test run: go up to the send button and stop. For the steps that
/// post something public a missing value means dry — only an explicit "0" posts —
/// so a step saved without the field can never publish by accident.
fn is_dry(p: &Value, vars: &HashMap<String, String>, public: bool) -> bool {
    match param(p, "dry").map(|v| expand(v, vars)).as_deref().map(str::trim) {
        Some("1") | Some("true") | Some("yes") => true,
        Some("0") | Some("false") | Some("no") => false,
        _ => public,
    }
}

// ---- the steps ---------------------------------------------------------------

/// One share of the post on the page. Returns the group it went to ("" for the profile).
#[allow(clippy::too_many_arguments)]
async fn share_once(
    mobile: bool,
    profile: &str,
    to: &str,
    caption: &str,
    dry: bool,
    entry: &str,
    done: &str,
    run: &Run,
) -> Result<String> {
    park(profile).await;
    find_and_click(mobile, profile, &js_share_button(), "share", "the Share button", 4).await?;
    pause(2000).await;
    // The pointer's click can be swallowed; if no dialog opened, press it from the page.
    if dialog_count(profile).await == "0" {
        let _ = script(profile, &js_share_press()).await;
        pause(2200).await;
    }

    let mut picked = String::new();
    if to == "profile" {
        let with_caption = !caption.trim().is_empty();
        if !with_caption && dry {
            // "Share now" posts on the spot, with no dialog to stop in, so
            // a dry run may look for the item but must not press it.
            let mut found = String::new();
            for _ in 0..5 {
                found = script(profile, &js_profile_item(false)).await?;
                if found.contains("HIR_OK") {
                    break;
                }
                pause(1200).await;
            }
            escape(profile).await;
            if !found.contains("HIR_OK") {
                return Err(anyhow!("Facebook: could not find \"Share now\" ({found}) — wrong link or Facebook's page changed"));
            }
            run.log("[dry] found \"Share now\", did not press it");
            return Ok(String::new());
        }
        if !with_caption {
            find_and_click(mobile, profile, &js_profile_item(false), "menu", "the \"Share now\" button", 5).await?;
            pause(2500).await;
            run.log("shared to the profile");
            return Ok(String::new());
        }
        // With a caption the Share dialog is itself the composer for the profile's feed:
        // the caption goes in, and "Chia sẻ ngay" below is the Post button.
    } else {
        if let Err(e) = find_and_click(mobile, profile, &js_group_item(), "menu", "the share-to-group item", 5).await {
            let d = script(profile, &js_menu_diag()).await.unwrap_or_default();
            escape(profile).await;
            return Err(anyhow!("{e} | dialog: {d}"));
        }
        pause(3000).await;
        if entry.is_empty() {
            // Any group: somewhere in the list, one not shared to yet.
            let _ = script(profile, &js_scroll_picker()).await;
            pause(1800).await;
        } else if script(profile, &js_search_box()).await?.contains("HIR_OK") {
            // A named group: type its name so it is on screen, whatever the list's length.
            let (x, y) = center_waiting(profile, "[data-hir=\"gsearch\"]", 6.0).await?;
            tap(mobile, profile, x, y).await?;
            pause(500).await;
            cdp::page_call(profile, "Motion.enterText", json!({ "text": entry })).await?;
            pause(2200).await;
        }
        let picker = script(profile, &js_group_picker_open()).await?;
        if !picker.contains("HIR_OK") {
            let d = script(profile, &js_menu_diag()).await.unwrap_or_default();
            escape(profile).await;
            escape(profile).await;
            return Err(anyhow!("Facebook: the group list did not open ({picker}), so nothing was picked | dialog: {d}"));
        }
        let what = if entry.is_empty() { "a group to share to".to_string() } else { format!("the group \"{entry}\"") };
        find_and_click(mobile, profile, &js_pick_group(entry, done), "pick", &what, 3).await?;
        picked = unquote(&script(profile, "document.documentElement.getAttribute('data-hir-name')||''").await.unwrap_or_default());
        pause(3000).await;
    }

    if !caption.trim().is_empty() {
        find_and_click(mobile, profile, &js_caption_box(), "caption", "the caption box", 4).await?;
        pause(600).await;
        cdp::page_call(profile, "Motion.enterText", json!({ "text": caption })).await?;
        pause(800).await;
    }

    let mut post = script(profile, &js_post_button()).await?;
    if !post.contains("HIR_OK") {
        pause(4000).await;
        post = script(profile, &js_post_button()).await?;
    }
    if !post.contains("HIR_OK") {
        escape(profile).await;
        escape(profile).await;
        return Err(anyhow!("Facebook: the Post button is not there or not ready ({post}) — the group may need another step (approval, topic…)"));
    }
    let where_to = if picked.is_empty() { to.to_string() } else { picked.clone() };
    if dry {
        escape(profile).await;
        escape(profile).await;
        run.log(format!("[dry] would post to {where_to}"));
    } else {
        let (x, y) = center_waiting(profile, "[data-hir=\"post\"]", 8.0).await?;
        tap(mobile, profile, x, y).await?;
        let mut closed = false;
        for _ in 0..30 {
            pause(1000).await;
            let n = script(profile, "String(document.querySelectorAll('div[role=\"dialog\"]').length)")
                .await
                .unwrap_or_default();
            if n.trim_matches('"') == "0" {
                closed = true;
                break;
            }
        }
        if !closed {
            return Err(anyhow!("pressed Post but the dialog did not close — the group may need approval or another step"));
        }
        run.log(format!("shared to {where_to}"));
    }
    Ok(picked)
}

/// A page-side string comes back JSON-quoted; this is the text inside.
fn unquote(s: &str) -> String {
    let t = s.trim();
    serde_json::from_str::<String>(t).unwrap_or_else(|_| t.to_string())
}

pub(super) async fn run(
    mobile: bool,
    profile: &str,
    kind: &str,
    p: &Value,
    vars: &mut HashMap<String, String>,
    run: &Run,
) -> Result<Flow> {
    match kind {
        "fb.check" => {
            guard(profile).await?;
            if script(profile, &js_logged_out()).await?.contains("HIR_LOGGED_OUT") {
                return Err(anyhow!(
                    "this profile is not logged in to Facebook — log in by hand once, then run again"
                ));
            }
            run.log("Facebook: logged in, no checkpoint");
        }

        "fb.watch" => {
            let base = num(p, "seconds", vars).unwrap_or(10.0).clamp(0.0, 3600.0);
            let extra = num(p, "extra", vars).unwrap_or(0.0).clamp(0.0, 3600.0);
            let total = base + extra * unit();
            let r = script(profile, &js_play()).await.unwrap_or_default();
            run.log(format!("watching for {:.0}s ({})", total, r.trim_start_matches("HIR_").to_lowercase()));
            let mut left = total;
            while left > 0.0 {
                let chunk = (2.5 + 2.0 * unit()).min(left);
                tokio::time::sleep(Duration::from_secs_f64(chunk)).await;
                left -= chunk;
                if left > 0.5 && !mobile {
                    // A hand resting on the mouse is never perfectly still.
                    let (cx, cy) = viewport_center(profile).await;
                    let (x, y) = (cx + (unit() - 0.5) * cx * 0.7, cy + (unit() - 0.5) * cy * 0.7);
                    let _ = cdp::page_call(profile, "Motion.createPointer", json!({ "x": x, "y": y })).await;
                    let _ = cdp::page_call(profile, "Motion.glideTo", json!({ "x": x, "y": y })).await;
                }
            }
        }

        "fb.react" => {
            let which = param(p, "reaction").map(|v| expand(v, vars).to_lowercase()).unwrap_or_else(|| "like".into());
            let dry = is_dry(p, vars, false);
            if which == "like" {
                let tagged = script(profile, &js_like()).await?;
                if tagged.contains("HIR_DONE") {
                    run.log("already reacted — left as it is");
                    return Ok(Flow::Next);
                }
                if !tagged.contains("HIR_OK") {
                    return Err(anyhow!("Facebook: could not find the Like button ({tagged}) — wrong link or Facebook's page changed"));
                }
                if dry {
                    run.log("[dry] would press Like");
                    return Ok(Flow::Next);
                }
                let (x, y) = center_waiting(profile, "[data-hir=\"like\"]", 8.0).await?;
                tap(mobile, profile, x, y).await?;
                run.log("liked");
                park(profile).await;
            } else {
                if mobile {
                    return Err(anyhow!("a phone has no cursor to rest on Like — only the plain Like works on this profile"));
                }
                let state = script(profile, &js_like()).await?;
                if let Some(label) = state.strip_prefix("HIR_DONE:") {
                    let label = label.to_lowercase();
                    if reaction_words(&which).iter().any(|w| label.contains(w)) && which != "like" {
                        run.log(format!("already reacted with {which} — left as it is"));
                        return Ok(Flow::Next);
                    }
                    run.log(format!("changing the reaction ({}) to {which}", label.trim()));
                }
                let tagged = script(profile, &js_react_anchor()).await?;
                if !tagged.contains("HIR_OK") {
                    return Err(anyhow!("Facebook: could not find the reaction button ({tagged}) — wrong link or Facebook's page changed"));
                }
                let (x, y) = center_waiting(profile, "[data-hir=\"like\"]", 8.0).await?;
                let mut last = String::new();
                // Pressing the reaction button opens the bar reliably; resting the
                // pointer on Like is the fallback (feed posts have no such button).
                if script(profile, &js_react_button()).await?.contains("HIR_OK") {
                    let (rx0, ry0) = center_waiting(profile, "[data-hir=\"rbtn\"]", 8.0).await?;
                    tap(mobile, profile, rx0, ry0).await?;
                    pause(1300).await;
                    last = script(profile, &js_reaction(&which)).await?;
                }
                for attempt in 0..(if last.contains("HIR_OK") { 0 } else { 4 }) {
                    // Rest the pointer on the button by moving onto it from the side:
                    // a pointer placed straight on it, or already there, sends no
                    // "entered" event and the bar never opens.
                    let (sx, sy) = (x - 140.0 - 25.0 * attempt as f64, y - 30.0);
                    cdp::page_call(profile, "Motion.createPointer", json!({ "x": sx, "y": sy })).await?;
                    cdp::page_call(profile, "Motion.glideTo", json!({ "x": x, "y": y })).await?;
                    pause(1500).await;
                    last = script(profile, &js_reaction(&which)).await?;
                    if last.contains("HIR_OK") {
                        break;
                    }
                }
                if !last.contains("HIR_OK") {
                    return Err(anyhow!("Facebook: the reaction bar did not open or has no \"{which}\" ({last})"));
                }
                if dry {
                    run.log(format!("[dry] would pick {which}"));
                    let (cx, cy) = viewport_center(profile).await;
                    let _ = cdp::page_call(profile, "Motion.glideTo", json!({ "x": cx, "y": cy })).await;
                    return Ok(Flow::Next);
                }
                let (rx, ry) = center_waiting(profile, "[data-hir=\"react\"]", 5.0).await?;
                tap(mobile, profile, rx, ry).await?;
                run.log(format!("reacted: {which}"));
                park(profile).await;
            }
        }

        "fb.comment" => {
            let text = expand(param(p, "text").unwrap_or(""), vars);
            if text.trim().is_empty() {
                // A spreadsheet's comment column is allowed to be blank on some rows.
                run.log("no comment text for this one — skipped");
                return Ok(Flow::Next);
            }
            let dry = is_dry(p, vars, true);
            park(profile).await;
            // A reel or video keeps the box hidden until its Comment button is pressed.
            if !script(profile, &js_comment_box()).await?.contains("HIR_OK") {
                // 1) a real click on the button
                if script(profile, &js_comment_button()).await?.contains("HIR_OK") {
                    let (x, y) = center_waiting(profile, "[data-hir=\"cbtn\"]", 8.0).await?;
                    tap(mobile, profile, x, y).await?;
                    pause(2200).await;
                }
                // 2) from the page, if that did not open it
                if !script(profile, &js_comment_box()).await?.contains("HIR_OK") {
                    let _ = script(profile, &js_comment_press()).await;
                    pause(2200).await;
                }
                // 3) one more real click, in case the first only closed a panel that was opening
                if !script(profile, &js_comment_box()).await?.contains("HIR_OK")
                    && script(profile, &js_comment_button()).await?.contains("HIR_OK")
                {
                    let (x, y) = center_waiting(profile, "[data-hir=\"cbtn\"]", 8.0).await?;
                    tap(mobile, profile, x, y).await?;
                    pause(2200).await;
                }
            }
            if let Err(e) = find_and_click(mobile, profile, &js_comment_box(), "cbox", "the comment box", 4).await {
                let d = script(profile, &js_comment_diag()).await.unwrap_or_default();
                return Err(anyhow!("{e} | page: {d}"));
            }
            pause(700).await;
            cdp::page_call(profile, "Motion.enterText", json!({ "text": text })).await?;
            pause(800).await;
            if dry {
                run.log("[dry] typed the comment, did not send it");
                return Ok(Flow::Next);
            }
            cdp::page_call(profile, "Motion.pressKey", json!({ "key": "Enter" })).await?;
            pause(1800).await;
            run.log("commented");
        }

        "fb.share" => {
            let to = param(p, "to").map(|v| expand(v, vars).to_lowercase()).unwrap_or_else(|| "group".into());
            let caption = expand(param(p, "caption").unwrap_or(""), vars);
            let dry = is_dry(p, vars, true);
            // Groups to choose from: names separated by new lines, commas or "|".
            // Empty = any group the account is in.
            let wanted: Vec<String> = expand(param(p, "group").unwrap_or(""), vars)
                .split(|c| c == '\n' || c == ',' || c == '|')
                .map(|g| g.trim().to_string())
                .filter(|g| !g.is_empty())
                .collect();
            let count = num(p, "count", vars).unwrap_or(1.0).clamp(1.0, 50.0) as usize;
            let gap = num(p, "gap", vars).unwrap_or(25.0).clamp(0.0, 3600.0);

            // Each post starts with nothing shared yet; the same post later in the
            // run (another pass) must not repeat the groups it already went to.
            let here = unquote(&script(profile, "location.href.split('?')[0]").await.unwrap_or_default());
            if vars.get("fb_done_for").map(String::as_str) != Some(here.as_str()) {
                vars.insert("fb_done_for".into(), here);
                vars.remove("fb_done_groups");
                vars.remove("fb_done_list");
            }

            let rounds = if to == "profile" { 1 } else { count };
            let mut shared = 0usize;
            for round in 0..rounds {
                if round > 0 {
                    // People do not share to five groups in five seconds.
                    pause((gap * (0.7 + 0.6 * unit()) * 1000.0) as u64).await;
                }
                let entry = if wanted.is_empty() {
                    String::new()
                } else {
                    let done_list = vars.get("fb_done_list").cloned().unwrap_or_default();
                    let done_entries: Vec<&str> = done_list.split("||").collect();
                    let left: Vec<&String> = wanted.iter().filter(|w| !done_entries.contains(&w.as_str())).collect();
                    if left.is_empty() {
                        run.log(format!("every group in the list has been shared to ({shared} this time)"));
                        break;
                    }
                    left[((unit() * left.len() as f64) as usize).min(left.len() - 1)].clone()
                };
                let done = vars.get("fb_done_groups").cloned().unwrap_or_default();
                let picked = share_once(mobile, profile, &to, &caption, dry, &entry, &done, run).await?;
                shared += 1;
                if !entry.is_empty() {
                    let list = vars.entry("fb_done_list".into()).or_default();
                    if !list.is_empty() {
                        list.push_str("||");
                    }
                    list.push_str(&entry);
                }
                if !picked.is_empty() {
                    let list = vars.entry("fb_done_groups".into()).or_default();
                    if !list.is_empty() {
                        list.push_str("||");
                    }
                    list.push_str(&picked);
                    if let Some(into) = param(p, "into") {
                        vars.insert(super::check_var_name(into)?, picked);
                    }
                }
            }
            run.log(format!("share step done: {shared} share(s)"));
        }

        "fb.save" => {
            let dry = is_dry(p, vars, false);
            park(profile).await;
            let direct = script(profile, &js_save_button()).await?;
            if direct.contains("HIR_DONE") {
                run.log("already saved — left as it is");
                return Ok(Flow::Next);
            }
            if direct.contains("HIR_OK") {
                if dry {
                    run.log("[dry] found the Save button, did not press it");
                    return Ok(Flow::Next);
                }
                let (x, y) = center_waiting(profile, "[data-hir=\"savebtn\"]", 8.0).await?;
                tap(mobile, profile, x, y).await?;
                pause(1200).await;
                run.log("saved");
                return Ok(Flow::Next);
            }
            find_and_click(mobile, profile, &js_more_button(), "more", "the post's \"…\" menu", 4).await?;
            pause(1200).await;
            let r = script(profile, &js_save_item()).await?;
            if r.contains("HIR_DONE") {
                run.log("already saved — left as it is");
                escape(profile).await;
                return Ok(Flow::Next);
            }
            if !r.contains("HIR_OK") {
                escape(profile).await;
                return Err(anyhow!("Facebook: no \"Save\" item in the menu ({r}) — wrong link or Facebook's page changed"));
            }
            if dry {
                escape(profile).await;
                run.log("[dry] would save it");
                return Ok(Flow::Next);
            }
            let (x, y) = center_waiting(profile, "[data-hir=\"save\"]", 5.0).await?;
            tap(mobile, profile, x, y).await?;
            pause(1000).await;
            run.log("saved");
        }

        other => return Err(anyhow!("unknown Facebook step {other}")),
    }
    Ok(Flow::Next)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A finder with a missing brace is a page-side syntax error that only shows
    /// up against a live Facebook page; catch the cheap kind here.
    #[test]
    fn every_finder_is_balanced_and_has_no_template_leftovers() {
        let all = [
            js_guard(), js_logged_out(), js_play(), js_like(), js_comment_box(), js_share_button(),
            js_group_item(), js_profile_item(true), js_profile_item(false), js_scroll_picker(),
            js_pick_group("Nhóm A", "x||y"), js_pick_group("", ""), js_caption_box(), js_post_button(),
            js_more_button(), js_save_item(), js_search_box(), js_comment_diag(), js_share_press(), js_menu_diag(), js_group_picker_open(), js_comment_button(), js_save_button(), js_react_button(), js_comment_press(), js_react_anchor(), js_reaction("love"), js_reaction("angry"),
        ];
        for s in &all {
            for (o, c) in [('(', ')'), ('{', '}'), ('[', ']')] {
                assert_eq!(s.matches(o).count(), s.matches(c).count(), "unbalanced {o}{c} in {s}");
            }
            assert!(!s.contains("{{") && !s.contains("}}"), "stray template brace in {s}");
        }
    }

    #[test]
    fn a_group_name_cannot_break_out_of_its_string() {
        let s = js_pick_group("a'b\\c\nd", "x'y");
        assert!(s.contains("a\\'b\\\\c d"));
        assert!(s.contains("x\\'y"));
    }
}
