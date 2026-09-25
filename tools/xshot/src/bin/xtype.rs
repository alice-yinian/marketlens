//! 开发用输入注入工具（不参与应用构建）。
//!
//! 为什么需要它：开发机是 AlmaLinux 10，RHEL 10 已移除 X.Org 服务器，
//! 也没有 `xdotool` / `xautomation` 可用。而「真实走一遍引导流程」这件事
//! 是 mock 测试覆盖不到的——jsdom 测试把 IPC 全 mock 掉了，
//! **前后端参数形状不匹配这类问题只有在真实 IPC 上才暴露**。
//!
//! 所以这里直接用 XTEST 扩展注入键鼠事件，配合 `xshot` 截图，
//! 就能在无头环境里驱动完整 UI 流程。
//!
//! 用法（需 DISPLAY 指向 Xwayland）：
//!   xtype text "some text"     逐字符输入
//!   xtype key Return           按一个键（X11 keysym 名）
//!   xtype click 640 400        在坐标处点击（左键）

use std::collections::HashMap;
use std::env;
use std::thread::sleep;
use std::time::Duration;

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt, KEY_PRESS_EVENT, KEY_RELEASE_EVENT};
use x11rb::protocol::xtest::ConnectionExt as XtestExt;
use x11rb::rust_connection::RustConnection;
use x11rb::NONE;

/// 注入事件之间留一点间隔：WebKit 处理输入事件需要时间，
/// 连发会丢字符（实测过）。
const KEY_GAP: Duration = Duration::from_millis(40);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(command) = args.first() else {
        eprintln!(
            "用法: xtype text <字符串> | xtype key <keysym> | xtype click <x> <y> | \
             xtype scroll <n> | xtype geom"
        );
        std::process::exit(2);
    };

    let (conn, _screen) = RustConnection::connect(None)?;

    // 先确认服务端支持 XTEST，否则后面每个请求都会失败得莫名其妙
    conn.xtest_get_version(2, 2)?.reply()?;

    match command.as_str() {
        "text" => {
            let text = args.get(1).map(String::as_str).unwrap_or("");
            type_text(&conn, text)?;
        }
        "key" => {
            let name = args.get(1).map(String::as_str).unwrap_or("Return");
            press_named_key(&conn, name)?;
        }
        "click" => {
            let x: i16 = args.get(1).and_then(|v| v.parse().ok()).unwrap_or(0);
            let y: i16 = args.get(2).and_then(|v| v.parse().ok()).unwrap_or(0);
            click(&conn, x, y)?;
        }
        // 打印可映射窗口在 root 上的位置。
        //
        // 必需：`xshot` 抓的是**子窗口**（应用窗口），而 XTEST 注入用的是
        // **root 坐标**。两者差一个窗口偏移量，而窗口位置由窗口管理器决定，
        // 每次启动可能不同——写死坐标会在某次运行后突然失效，且症状是
        // 「点了没反应」，极难联想到坐标问题。
        "geom" => {
            let screen = &conn.setup().roots[0];
            let tree = conn.query_tree(screen.root)?.reply()?;
            for &child in &tree.children {
                let Ok(attrs) = conn.get_window_attributes(child)?.reply() else {
                    continue;
                };
                if attrs.map_state != x11rb::protocol::xproto::MapState::VIEWABLE {
                    continue;
                }
                let geo = conn.get_geometry(child)?.reply()?;
                let translated = conn
                    .translate_coordinates(child, screen.root, 0, 0)?
                    .reply()?;
                println!(
                    "window={child} size={}x{} root_offset=({}, {})",
                    geo.width, geo.height, translated.dst_x, translated.dst_y
                );
            }
        }
        "scroll" => {
            // 负数向上滚（button 4），正数向下（button 5）
            let count: i32 = args.get(1).and_then(|v| v.parse().ok()).unwrap_or(3);
            // 可选坐标：双栏布局里左栏和右栏是**各自独立**的滚动容器，
            // 固定滚窗口中心只能滚到左栏。不给坐标时保持旧行为。
            let at = match (
                args.get(2).and_then(|v| v.parse::<i16>().ok()),
                args.get(3).and_then(|v| v.parse::<i16>().ok()),
            ) {
                (Some(x), Some(y)) => Some((x, y)),
                _ => None,
            };
            scroll(&conn, count, at)?;
        }
        other => {
            eprintln!("未知子命令：{other}");
            std::process::exit(2);
        }
    }

    Ok(())
}

/// 逐字符输入。需要把字符映射到 keycode 与是否需要 Shift。
fn type_text(conn: &RustConnection, text: &str) -> Result<(), Box<dyn std::error::Error>> {
    let map = keyboard_map(conn)?;
    // 注入事件要的是 **keycode**，不是 keysym——Shift_L 的 keysym 是 0xFFE1，
    // 但它的 keycode 由布局决定，必须查表。
    let shift_keycode = keycode_for_keysym(conn, 0xFFE1)?;

    for ch in text.chars() {
        let Some(&(keycode, needs_shift)) = map.get(&ch) else {
            return Err(format!("当前键盘布局打不出字符 {ch:?}").into());
        };

        if needs_shift {
            fake_key(conn, shift_keycode, KEY_PRESS_EVENT)?;
        }
        fake_key(conn, keycode, KEY_PRESS_EVENT)?;
        fake_key(conn, keycode, KEY_RELEASE_EVENT)?;
        if needs_shift {
            fake_key(conn, shift_keycode, KEY_RELEASE_EVENT)?;
        }
        conn.flush()?;
        sleep(KEY_GAP);
    }
    Ok(())
}

fn press_named_key(conn: &RustConnection, name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let keysym = match name {
        "Return" | "Enter" => 0xFF0D,
        "Tab" => 0xFF09,
        "Escape" => 0xFF1B,
        "space" => 0x0020,
        "BackSpace" => 0xFF08,
        other => {
            // 单字符按字面处理，否则视为未知键名
            let mut chars = other.chars();
            match (chars.next(), chars.next()) {
                (Some(ch), None) => ch as u32,
                _ => return Err(format!("未知键名：{other}").into()),
            }
        }
    };

    let keycode = keycode_for_keysym(conn, keysym)?;
    fake_key(conn, keycode, KEY_PRESS_EVENT)?;
    fake_key(conn, keycode, KEY_RELEASE_EVENT)?;
    conn.flush()?;
    sleep(KEY_GAP);
    Ok(())
}

fn click(conn: &RustConnection, x: i16, y: i16) -> Result<(), Box<dyn std::error::Error>> {
    // 先移动指针再按下，否则点击会落在指针原来的位置
    conn.xtest_fake_input(x11rb::protocol::xproto::MOTION_NOTIFY_EVENT, 0, 0, NONE, x, y, 0)?;
    conn.flush()?;
    sleep(Duration::from_millis(80));

    conn.xtest_fake_input(x11rb::protocol::xproto::BUTTON_PRESS_EVENT, 1, 0, NONE, 0, 0, 0)?;
    conn.xtest_fake_input(x11rb::protocol::xproto::BUTTON_RELEASE_EVENT, 1, 0, NONE, 0, 0, 0)?;
    conn.flush()?;
    sleep(Duration::from_millis(120));
    Ok(())
}

/// 滚轮滚动。指针需要先位于可滚动区域上方。
///
/// X11 的滚轮是鼠标按钮 4（上）/ 5（下），不是独立事件类型。
fn scroll(conn: &RustConnection, count: i32, at: Option<(i16, i16)>) -> Result<(), Box<dyn std::error::Error>> {
    // 先把指针移到目标位置，否则滚动会落在指针当前所在的元素上
    let (x, y) = at.unwrap_or((680, 500));
    conn.xtest_fake_input(
        x11rb::protocol::xproto::MOTION_NOTIFY_EVENT,
        0,
        0,
        NONE,
        x,
        y,
        0,
    )?;
    conn.flush()?;
    sleep(Duration::from_millis(100));

    let button = if count < 0 { 4 } else { 5 };
    for _ in 0..count.unsigned_abs() {
        conn.xtest_fake_input(x11rb::protocol::xproto::BUTTON_PRESS_EVENT, button, 0, NONE, 0, 0, 0)?;
        conn.xtest_fake_input(x11rb::protocol::xproto::BUTTON_RELEASE_EVENT, button, 0, NONE, 0, 0, 0)?;
        conn.flush()?;
        sleep(Duration::from_millis(80));
    }
    sleep(Duration::from_millis(300));
    Ok(())
}

fn fake_key(
    conn: &RustConnection,
    keycode: u8,
    event_type: u8,
) -> Result<(), Box<dyn std::error::Error>> {
    conn.xtest_fake_input(event_type, keycode, 0, NONE, 0, 0, 0)?;
    Ok(())
}

/// 从当前键盘布局构建「字符 → (keycode, 是否需要 Shift)」。
///
/// 不硬编码键位表：布局可能是任意一种，硬编码会在非 US 布局下静默打错字符。
fn keyboard_map(conn: &RustConnection) -> Result<HashMap<char, (u8, bool)>, Box<dyn std::error::Error>> {
    let setup = conn.setup();
    let min_keycode = setup.min_keycode;
    let count = setup.max_keycode - min_keycode + 1;
    let reply = conn.get_keyboard_mapping(min_keycode, count)?.reply()?;
    let per_keycode = usize::from(reply.keysyms_per_keycode);

    let mut map: HashMap<char, (u8, bool)> = HashMap::new();
    for (index, chunk) in reply.keysyms.chunks(per_keycode).enumerate() {
        let keycode = min_keycode + index as u8;
        // level 0 = 无修饰，level 1 = Shift
        for (level, &keysym) in chunk.iter().take(2).enumerate() {
            let Some(ch) = char::from_u32(keysym) else {
                continue;
            };
            if ch.is_control() {
                continue;
            }
            // 先到先得：不覆盖已存在的映射，保证结果稳定
            map.entry(ch).or_insert((keycode, level == 1));
        }
    }
    Ok(map)
}

fn keycode_for_keysym(
    conn: &RustConnection,
    keysym: u32,
) -> Result<u8, Box<dyn std::error::Error>> {
    let setup = conn.setup();
    let min_keycode = setup.min_keycode;
    let count = setup.max_keycode - min_keycode + 1;
    let reply = conn.get_keyboard_mapping(min_keycode, count)?.reply()?;
    let per_keycode = usize::from(reply.keysyms_per_keycode);

    for (index, chunk) in reply.keysyms.chunks(per_keycode).enumerate() {
        if chunk.iter().any(|&ks| ks == keysym) {
            return Ok(min_keycode + index as u8);
        }
    }
    Err(format!("键盘布局里找不到 keysym 0x{keysym:04X}").into())
}
