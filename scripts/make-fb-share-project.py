#!/usr/bin/env python3
"""Writes docs/automation/facebook-chia-se-nhom.json — an automation project
(Hir-Login "bundle" format 1) that takes post links from an Excel sheet, opens
each one in a logged-in Facebook profile and shares it to random groups the
account has already joined, a few per link.

Run it again after editing the steps below; import the JSON in Tự động hoá → Nhập.
"""
import json, os, sys, time

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "docs", "automation", "facebook-chia-se-nhom.json")

# ---- page-side helpers --------------------------------------------------------
# Facebook's class names are generated and change; aria-labels and visible text
# are what a person reads, so that is what the steps look for. Each script finds
# an element and tags it with data-hir=…; the click that follows is a normal
# (trusted) click on the tag, not a scripted .click().
PRE = (
    "var vis=function(e){var r=e.getBoundingClientRect();if(r.width<3||r.height<3)return false;"
    "var s=getComputedStyle(e);return s.visibility!=='hidden'&&s.display!=='none';};"
    "var txt=function(e){return (e.innerText||'').replace(/\\s+/g,' ').trim();};"
    "var topDlg=function(){var d=[].slice.call(document.querySelectorAll('div[role=\"dialog\"]')).filter(vis);"
    "var a=function(e){var r=e.getBoundingClientRect();return r.width*r.height;};"
    "d.sort(function(x,y){return a(y)-a(x);});return d.length?d[0]:document.body;};"
    "var clear=function(){[].slice.call(document.querySelectorAll('[data-hir]')).forEach("
    "function(e){e.removeAttribute('data-hir');});};"
)

JS_GUARD = (
    "(function(){" + PRE +
    "var t=((document.body&&document.body.innerText)||'').slice(0,30000);"
    "var bad=/(tạm thời bị chặn|temporarily blocked|bạn tạm thời không thể|hãy chậm lại|"
    "xác nhận danh tính|confirm your identity|xác thực hai yếu tố|two-factor|"
    "tài khoản của bạn đã bị hạn chế|your account has been restricted)/i;"
    "return (bad.test(t)||/checkpoint|two_step/.test(location.href))?'HIR_BLOCKED':'HIR_OK';})()"
)

JS_TAG_SHARE = (
    "(function(){" + PRE + "clear();"
    "var re=/(gửi nội dung này cho bạn bè|send this to friends|^chia sẻ$|^share$)/i;"
    "var c=[].slice.call(document.querySelectorAll('[role=\"button\"]')).filter(function(e){"
    "return vis(e)&&!e.closest('div[role=\"dialog\"]')&&(re.test(e.getAttribute('aria-label')||'')||re.test(txt(e)));});"
    "if(!c.length)return 'HIR_NONE';"
    "c[0].setAttribute('data-hir','share');c[0].scrollIntoView({block:'center'});return 'HIR_OK';})()"
)

JS_TAG_GROUP_ITEM = (
    "(function(){" + PRE + "clear();var root=topDlg();"
    "var re=/^(chia sẻ (lên|vào) nhóm|nhóm$|share to a group|share in a group|groups?$)/i;"
    "var c=[].slice.call(root.querySelectorAll('[role=\"button\"],[role=\"menuitem\"],[role=\"link\"],[tabindex]'))"
    ".filter(function(e){return vis(e)&&re.test(txt(e));});"
    "if(!c.length)return 'HIR_NONE';"
    "c.sort(function(a,b){return txt(a).length-txt(b).length;});"
    "c[0].setAttribute('data-hir','menu');return 'HIR_OK';})()"
)

JS_SCROLL_PICKER = (
    "(function(){" + PRE + "var root=topDlg();"
    "var rows=[].slice.call(root.querySelectorAll('[role=\"button\"],[role=\"radio\"],[role=\"option\"],[role=\"listitem\"]')).filter(vis);"
    "if(!rows.length)return 'HIR_NONE';"
    "var p=rows[0].parentElement;"
    "while(p&&p!==document.body){if(p.scrollHeight>p.clientHeight+30){var o=getComputedStyle(p).overflowY;"
    "if(o==='auto'||o==='scroll')break;}p=p.parentElement;}"
    "if(p&&p!==document.body){p.scrollTop=Math.floor(Math.random()*Math.max(1,p.scrollHeight-p.clientHeight)*0.85);return 'HIR_SCROLLED';}"
    "return 'HIR_FLAT';})()"
)

JS_PICK_GROUP = (
    "(function(){" + PRE + "clear();var done=('{{done_groups}}').split('||');var root=topDlg();"
    "var skip=/^(đăng|post|hủy|huỷ|cancel|đóng|close|quay lại|back|tìm kiếm|search|xong|done|"
    "chia sẻ lên nhóm|share to a group|chia sẻ|share)$/i;"
    "var seen={};var c=[];"
    "[].slice.call(root.querySelectorAll('[role=\"button\"],[role=\"radio\"],[role=\"option\"],[role=\"listitem\"]')).forEach(function(e){"
    "if(!vis(e))return;"
    "var name=((e.innerText||'').split('\\n')[0]||'').replace(/['\"\\\\]/g,'').trim();"
    "if(name.length<2||name.length>80||skip.test(name)||seen[name]||done.indexOf(name)>=0)return;"
    "seen[name]=1;c.push([e,name]);});"
    "if(!c.length)return 'HIR_NONE';"
    "var k=c[Math.floor(Math.random()*c.length)];"
    "k[0].setAttribute('data-hir','pick');k[0].scrollIntoView({block:'center'});"
    "document.documentElement.setAttribute('data-hir-name',k[1]);return 'HIR_OK';})()"
)

JS_TAG_POST = (
    "(function(){" + PRE + "clear();var root=topDlg();var re=/^(đăng|post|chia sẻ|share)$/i;"
    "var c=[].slice.call(root.querySelectorAll('[role=\"button\"]')).filter(function(e){"
    "return vis(e)&&(re.test((e.getAttribute('aria-label')||'').trim())||re.test(txt(e)));});"
    "if(!c.length)return 'HIR_NONE';var b=c[c.length-1];"
    "if(b.getAttribute('aria-disabled')==='true')return 'HIR_DISABLED';"
    "b.setAttribute('data-hir','post');return 'HIR_OK';})()"
)

for name, js in [("GUARD", JS_GUARD), ("TAG_SHARE", JS_TAG_SHARE), ("TAG_GROUP_ITEM", JS_TAG_GROUP_ITEM),
                 ("SCROLL_PICKER", JS_SCROLL_PICKER), ("PICK_GROUP", JS_PICK_GROUP), ("TAG_POST", JS_TAG_POST)]:
    # The runner replaces {{name}} for known variables; anything else with a
    # double brace in a script would be a bug waiting for a variable of that name.
    stripped = js.replace("{{done_groups}}", "")
    assert "{{" not in stripped, f"{name}: stray double brace"

# ---- the steps ----------------------------------------------------------------
blocks = []

def block(bid, kind, label, params, done="next", fail="stop", secrets=None):
    blocks.append({
        "id": bid, "kind": kind, "label": label, "params": params, "enabled": True,
        "x": 0.0, "y": float(len(blocks) * 90),
        "on_done": done, "on_fail": fail, "secrets": secrets or [],
    })

def goto(target): return {"goto": target}

def var(bid, label, name, value, secrets=None):
    block(bid, "var.set", label, {"name": name, "value": value}, secrets=secrets)

# 1. What the operator edits.
var("v_home", "① Trang chủ Facebook (đổi nếu cần)", "home_url", "https://www.facebook.com/")
var("v_excel", "① File Excel chứa link bài viết", "excel_path", "C:\\link\\bai-viet.xlsx")
var("v_col", "① Tên cột chứa link (dòng đầu file)", "excel_column", "link")
var("v_profile", "① ID profile Facebook đã đăng nhập", "profile_id", "DAN-ID-PROFILE-VAO-DAY")
var("v_per", "① Số nhóm mỗi link", "per_run", "5")
var("v_watch", "① Xem bài/reel bao lâu trước khi chia sẻ (giây)", "watch_s", "12")
var("v_gmin", "① Nghỉ giữa 2 lần chia sẻ — ít nhất (giây)", "gap_min", "25")
var("v_gmax", "① Nghỉ giữa 2 lần chia sẻ — nhiều nhất (giây)", "gap_max", "70")
var("v_dry", "① Chạy THỬ không đăng thật? (1 = thử: đi hết các bước nhưng KHÔNG bấm Đăng; 0 = đăng thật)", "dry_run", "1")
var("v_user", "① (Tuỳ chọn) Email/SĐT Facebook — chỉ dùng khi profile chưa đăng nhập", "fb_user", "")
var("v_pass", "① (Tuỳ chọn) Mật khẩu Facebook", "fb_pass", "", secrets=["value"])

# 2. Profile and the next link.
block("use_profile", "profile.use", "② Dùng profile", {"id": "{{profile_id}}"})
var("v_mode", "② Cách lấy link: đăng thật = mỗi link 1 lần", "take_mode", "next")
block("chk_dry_take", "if.value", "② Chạy thử? → chỉ xem link, không đánh dấu đã dùng", {"a": "{{dry_run}}", "op": "=", "b": "1"},
      done="next", fail=goto("take_link"))
var("v_mode_peek", "② (chạy thử) xem link, không đánh dấu", "take_mode", "peek")
block("take_link", "sheet.next", "② Lấy link bài viết kế tiếp từ Excel",
      {"path": "{{excel_path}}", "column": "{{excel_column}}", "mode": "{{take_mode}}", "into": "post_url"})
var("v_shared", "② Đặt lại bộ đếm", "shared", "0")
var("v_tag", "② Đặt lại nhãn nhật ký", "tag", "")
var("v_done", "② Đặt lại danh sách nhóm đã chia sẻ", "done_groups", "")

# 3. Facebook home, and are we logged in?
# `goto` already waits for the page to load; a separate waitLoad afterwards only
# waits for a *future* load event and burns its whole timeout.
block("open_home", "goto", "③ Mở Facebook", {"url": "{{home_url}}"})
block("home_wait", "wait", "③ Nghỉ", {"seconds": 4})
block("guard1", "script.run", "③ Kiểm tra Facebook có đang chặn/đòi xác minh không",
      {"source": JS_GUARD, "world": "isolated", "into": "guard"})
block("chk_guard1", "if.value", "③ Đang bị chặn? → dừng", {"a": "{{guard}}", "op": "contains", "b": "HIR_BLOCKED"},
      done=goto("blocked_end"), fail="next")
block("chk_login", "if.exists", "③ Thấy ô mật khẩu (chưa đăng nhập)?", {"selector": "input[name=\"pass\"]"},
      done="next", fail=goto("go_post"))
block("chk_creds", "if.value", "③ Chưa điền email/mật khẩu? → dừng", {"a": "{{fb_user}}", "op": "is empty", "b": ""},
      done=goto("need_login"), fail="next")
block("type_user", "type", "③ Nhập email/SĐT", {"selector": "input[name=\"email\"]", "text": "{{fb_user}}"})
block("type_pass", "type", "③ Nhập mật khẩu", {"selector": "input[name=\"pass\"]", "text": "{{fb_pass}}"})
block("press_enter", "press", "③ Bấm Enter đăng nhập", {"key": "Enter"})
block("login_wait", "wait", "③ Chờ đăng nhập", {"seconds": 8})
block("guard2", "script.run", "③ Sau đăng nhập: có checkpoint/xác minh không?",
      {"source": JS_GUARD, "world": "isolated", "into": "guard"})
block("chk_guard2", "if.value", "③ Có → dừng (không cố vượt)", {"a": "{{guard}}", "op": "contains", "b": "HIR_BLOCKED"},
      done=goto("blocked_end"), fail="next")
block("chk_login2", "if.exists", "③ Vẫn thấy ô mật khẩu? → đăng nhập thất bại", {"selector": "input[name=\"pass\"]"},
      done=goto("login_failed"), fail="next")

# 4. Open the post.
block("go_post", "goto", "④ Mở bài viết", {"url": "{{post_url}}"})
block("post_wait", "wait", "④ Xem bài viết trước khi chia sẻ", {"seconds": "{{watch_s}}"})

# 5. One share, repeated per_run times.
block("loop", "if.value", "⑤ Đủ số nhóm chưa?", {"a": "{{shared}}", "op": ">=", "b": "{{per_run}}"},
      done=goto("finish"), fail="next")
block("guard_loop", "script.run", "⑤ Facebook có đang chặn không?",
      {"source": JS_GUARD, "world": "isolated", "into": "guard"})
block("chk_guard_loop", "if.value", "⑤ Có → dừng", {"a": "{{guard}}", "op": "contains", "b": "HIR_BLOCKED"},
      done=goto("blocked_end"), fail="next")

block("tag_share", "script.run", "⑤ Tìm nút Chia sẻ", {"source": JS_TAG_SHARE, "world": "isolated", "into": "r"})
block("chk_share", "if.value", "⑤ Thấy nút?", {"a": "{{r}}", "op": "contains", "b": "HIR_OK"}, done="next", fail=goto("fail_share"))
block("click_share", "click", "⑤ Bấm Chia sẻ", {"selector": "[data-hir=\"share\"]", "timeout": 8})
block("share_wait", "wait", "⑤ Nghỉ", {"seconds": 2})
if os.environ.get("FB_DBG"):
    block("dbg_s", "script.run", "DBG", {"source": "(function(){var d=[].slice.call(document.querySelectorAll('div[role=dialog]')).map(function(d){var r=d.getBoundingClientRect();return Math.round(r.width)+'x'+Math.round(r.height)});var e=document.querySelector('[data-hir=share]');var r=e&&e.getBoundingClientRect();return JSON.stringify({url:location.pathname,dlgs:d,share:r&&[Math.round(r.x),Math.round(r.y)],vis:document.visibilityState,sy:scrollY,nb:document.querySelectorAll('div[role=dialog] div[role=button]').length})})()", "world": "isolated", "into": "dbg"})
    block("dbg_l", "log", "DBG", {"text": "DBG {{dbg}}"})

block("tag_menu", "script.run", "⑤ Tìm mục \"Chia sẻ lên nhóm\"", {"source": JS_TAG_GROUP_ITEM, "world": "isolated", "into": "r"})
block("chk_menu", "if.value", "⑤ Thấy mục?", {"a": "{{r}}", "op": "contains", "b": "HIR_OK"}, done=goto("click_menu"), fail="next")
block("menu_retry_wait", "wait", "⑤ Chưa thấy — chờ thêm", {"seconds": 4})
block("tag_menu2", "script.run", "⑤ Tìm mục \"Chia sẻ lên nhóm\" (lần 2)", {"source": JS_TAG_GROUP_ITEM, "world": "isolated", "into": "r"})
block("chk_menu2", "if.value", "⑤ Thấy mục?", {"a": "{{r}}", "op": "contains", "b": "HIR_OK"}, done=goto("click_menu"), fail="next")
# The first click after a fresh page can be swallowed; press Chia sẻ once more.
block("tag_share_b", "script.run", "⑤ Tìm lại nút Chia sẻ", {"source": JS_TAG_SHARE, "world": "isolated", "into": "r"})
block("chk_share_b", "if.value", "⑤ Thấy nút?", {"a": "{{r}}", "op": "contains", "b": "HIR_OK"}, done="next", fail=goto("fail_menu"))
block("click_share_b", "click", "⑤ Bấm Chia sẻ (lần 2)", {"selector": "[data-hir=\"share\"]", "timeout": 8})
block("share_wait_b", "wait", "⑤ Nghỉ", {"seconds": 3})
block("tag_menu3", "script.run", "⑤ Tìm mục \"Chia sẻ lên nhóm\" (lần 3)", {"source": JS_TAG_GROUP_ITEM, "world": "isolated", "into": "r"})
block("chk_menu3", "if.value", "⑤ Thấy mục?", {"a": "{{r}}", "op": "contains", "b": "HIR_OK"}, done="next", fail=goto("fail_menu"))
block("click_menu", "click", "⑤ Bấm \"Chia sẻ lên nhóm\"", {"selector": "[data-hir=\"menu\"]", "timeout": 8})
block("menu_wait", "wait", "⑤ Chờ danh sách nhóm", {"seconds": 3})

block("scroll_list", "script.run", "⑤ Cuộn danh sách nhóm đến chỗ ngẫu nhiên", {"source": JS_SCROLL_PICKER, "world": "isolated", "into": "r"})
block("scroll_wait", "wait", "⑤ Chờ nhóm tải thêm", {"seconds": 2})
block("pick", "script.run", "⑤ Chọn ngẫu nhiên 1 nhóm chưa chia sẻ", {"source": JS_PICK_GROUP, "world": "isolated", "into": "r"})
block("chk_pick", "if.value", "⑤ Chọn được nhóm?", {"a": "{{r}}", "op": "contains", "b": "HIR_OK"}, done="next", fail=goto("fail_pick"))
block("read_name", "readAttribute", "⑤ Đọc tên nhóm đã chọn", {"selector": "html", "name": "data-hir-name", "into": "picked"})
block("click_pick", "click", "⑤ Bấm vào nhóm", {"selector": "[data-hir=\"pick\"]", "timeout": 8})
block("pick_wait", "wait", "⑤ Chờ khung đăng bài", {"seconds": 3})

block("tag_post", "script.run", "⑤ Tìm nút Đăng", {"source": JS_TAG_POST, "world": "isolated", "into": "r"})
block("chk_post", "if.value", "⑤ Nút Đăng sẵn sàng?", {"a": "{{r}}", "op": "contains", "b": "HIR_OK"}, done=goto("dry_check"), fail="next")
block("post_retry_wait", "wait", "⑤ Chưa sẵn sàng — chờ thêm", {"seconds": 4})
block("tag_post2", "script.run", "⑤ Tìm nút Đăng (lần 2)", {"source": JS_TAG_POST, "world": "isolated", "into": "r"})
block("chk_post2", "if.value", "⑤ Nút Đăng sẵn sàng?", {"a": "{{r}}", "op": "contains", "b": "HIR_OK"}, done="next", fail=goto("fail_post"))
block("dry_check", "if.value", "⑤ Đang chạy thử (không đăng)?", {"a": "{{dry_run}}", "op": "=", "b": "1"},
      done=goto("dry_close"), fail=goto("click_post"))
block("click_post", "click", "⑤ Bấm Đăng", {"selector": "[data-hir=\"post\"]", "timeout": 8})
block("post_closed", "waitGone", "⑤ Chờ khung đăng bài đóng", {"selector": "div[role=\"dialog\"]", "timeout": 30},
      done=goto("count"), fail=goto("fail_post"))

block("dry_close", "press", "⑤ [THỬ] Đóng khung đăng bài, KHÔNG đăng", {"key": "Escape"})
block("dry_close2", "press", "⑤ [THỬ] Đóng lần nữa nếu còn", {"key": "Escape"})
var("dry_tag", "⑤ [THỬ] Đánh dấu nhật ký là chạy thử", "tag", "[THỬ] ")
block("dry_say", "log", "⑤ [THỬ] Sẽ đăng vào nhóm", {"text": "[THỬ — không đăng] sẽ chia sẻ vào: {{picked}}"})
block("count", "var.math", "⑤ Đếm +1", {"a": "{{shared}}", "op": "+", "b": "1", "into": "shared"})
var("add_done", "⑤ Ghi nhớ nhóm này", "done_groups", "{{done_groups}}||{{picked}}")
block("write_log", "file.append", "⑤ Ghi file nhật ký", {"path": "{{excel_path}}.da-chia-se.txt", "line": "{{tag}}{{post_url}} -> {{picked}}"})
block("say", "log", "⑤ Báo tiến độ", {"text": "{{tag}}Đã chia sẻ {{shared}}/{{per_run}}: {{picked}}"})
block("gap_rand", "var.random", "⑤ Chọn thời gian nghỉ ngẫu nhiên", {"min": "{{gap_min}}", "max": "{{gap_max}}", "into": "gap"})
block("gap_wait", "wait", "⑤ Nghỉ giữa 2 nhóm", {"seconds": "{{gap}}"}, done=goto("loop"))

# 6. Done with this link — the next pass takes the next one.
block("finish", "log", "⑥ Xong link này", {"text": "Xong: {{shared}} nhóm cho {{post_url}}"})
block("end_pass", "stop", "⑥ Kết thúc lượt (lượt sau lấy link kế tiếp)", {})

# 7. Stop-and-say-why. Never pushes through a challenge.
def fail(bid, label, text): block(bid, "fail", label, {"text": text})
fail("blocked_end", "✖ Facebook chặn / đòi xác minh",
     "Facebook đang chặn hoặc đòi xác minh (checkpoint). Dừng, không cố vượt qua — vào profile xử lý bằng tay rồi chạy lại.")
fail("need_login", "✖ Chưa đăng nhập",
     "Profile chưa đăng nhập Facebook và chưa điền email/mật khẩu ở bước ①. Đăng nhập profile bằng tay (hoặc nhập cookie) rồi chạy lại.")
fail("login_failed", "✖ Đăng nhập thất bại", "Đăng nhập không thành công (vẫn thấy ô mật khẩu). Kiểm tra lại email/mật khẩu.")
fail("fail_share", "✖ Không thấy nút Chia sẻ", "Không thấy nút Chia sẻ trên bài viết — link sai, bài đã xoá, hoặc giao diện Facebook đã đổi.")
fail("fail_menu", "✖ Không thấy mục chia sẻ lên nhóm", "Không thấy mục chia sẻ lên nhóm — tài khoản chưa vào nhóm nào, hoặc giao diện Facebook đã đổi.")
fail("fail_pick", "✖ Không chọn được nhóm", "Không còn nhóm nào để chọn (đã chia sẻ hết các nhóm đang thấy) hoặc giao diện Facebook đã đổi.")
fail("fail_post", "✖ Đăng không thành công", "Không thấy nút Đăng, hoặc đã bấm Đăng nhưng khung đăng bài không đóng — nhóm có thể yêu cầu thêm bước (duyệt bài, chọn chủ đề...) hoặc giao diện Facebook đã đổi.")

NOTES = """Lấy link bài viết trong file Excel, mở bài đó bằng profile Facebook đã đăng nhập và chia sẻ vào ngẫu nhiên các nhóm tài khoản đã tham gia — mỗi link chia sẻ vào số nhóm đặt ở bước ① (mặc định 5), nghỉ ngẫu nhiên giữa các lần.

CÁCH DÙNG
1. Excel: 1 cột chứa link (dòng đầu là tên cột, mặc định "link"). File của bạn không bị sửa; link đã lấy được ghi vào file cùng tên có đuôi .used.txt để không chia sẻ lại — muốn dùng lại 1 link thì xoá dòng đó trong file .used.txt.
2. Sửa các ô ① (đường dẫn Excel, ID profile…).
   LẦN ĐẦU CHẠY THỬ: ô "Chạy THỬ" mặc định = 1 — đi hết các bước (mở bài, bấm Chia sẻ, chọn nhóm, mở khung đăng bài) nhưng KHÔNG bấm Đăng và không đánh dấu link đã dùng. Xem trình duyệt/nhật ký thấy đúng rồi mới đổi sang 0 để đăng thật.
3. Đặt "Số lượt" (loops) = số link muốn chạy. Mỗi lượt lấy 1 link kế tiếp; hết link thì lần chạy dừng.
4. Profile phải đăng nhập Facebook sẵn (nhập cookie hoặc đăng nhập tay một lần). Chỉ điền email/mật khẩu nếu thật sự cần.

NHẬT KÝ: mỗi nhóm đã chia sẻ được ghi vào <file Excel>.da-chia-se.txt (link -> tên nhóm).

AN TOÀN: gặp checkpoint, yêu cầu xác minh hay thông báo bị hạn chế là DỪNG ngay, không cố vượt qua. Facebook có thể giới hạn tài khoản chia sẻ nhiều/nhanh — đừng hạ thời gian nghỉ quá thấp, đừng tăng số nhóm quá cao.

LƯU Ý: giao diện Facebook hay đổi; các bước tìm nút dựa vào nhãn/chữ hiển thị (tiếng Việt và tiếng Anh). Nếu một bước báo không thấy nút, gửi lại log để chỉnh."""

bundle = {
    "format": 1,
    "exported_at": int(time.time()),
    "project": {
        "id": "fb-share-groups-template",
        "name": "Facebook — chia sẻ bài viết vào 5 nhóm ngẫu nhiên",
        "notes": NOTES,
        "blocks": blocks,
        "run": {"threads": 1, "loops": 1, "hours": 0.0, "profiles": [], "start": ""},
        "rules": [],
        "created_at": int(time.time()),
        "updated_at": int(time.time()),
    },
    "modules": [],
    "module_files": [],
    "needs": [
        {"block_id": "v_pass", "label": "① (Tuỳ chọn) Mật khẩu Facebook", "params": ["value"]},
    ],
}

ids = [b["id"] for b in blocks]
assert len(ids) == len(set(ids)), "duplicate block ids"
for b in blocks:
    for br in (b["on_done"], b["on_fail"]):
        if isinstance(br, dict) and "goto" in br:
            assert br["goto"] in ids, f"{b['id']} jumps to a missing block {br['goto']}"

out = os.path.normpath(OUT)
os.makedirs(os.path.dirname(out), exist_ok=True)
with open(out, "w", encoding="utf-8") as f:
    json.dump(bundle, f, ensure_ascii=False, indent=2)
print(f"wrote {out}: {len(blocks)} steps")
