//! 开发用截图工具（不参与应用构建）。
//!
//! 为什么需要它：开发机是 AlmaLinux 10，而 RHEL 10 已移除 X.Org 服务器，
//! 于是常规的截图路径全部不可用：
//!   * Xvfb            —— EL10 无此包
//!   * ImageMagick import —— EPEL 构建未包含 X11 支持（delegates 里没有 x）
//!   * xwd             —— 无此包
//!   * weston-screenshooter —— 服务端拒绝授权（unauthorized）
//!   * weston debug 的 screenshot 流 —— weston 14 已无此流
//!
//! 可行组合是 weston headless + Xwayland（提供真实 X display），
//! 再由本工具用 X11 的 GetImage 抓取窗口，输出 PPM（再用 ImageMagick 转 PNG）。
//!
//! 注意：Xwayland 的 root 窗口在 GetImage 上会返回 BadMatch，所以这里优先
//! 抓取面积最大的可映射子窗口（通常就是应用主窗口）。
//!
//! 用法：xshot [输出.ppm]   （需要 DISPLAY 指向 Xwayland）

use std::env;
use std::fs::File;
use std::io::{BufWriter, Write};

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt, ImageFormat, MapState, Window};
use x11rb::rust_connection::RustConnection;

/// 抓取指定窗口并写成 PPM(P6)。
fn capture(
    conn: &RustConnection,
    win: Window,
    label: &str,
    out: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let geo = conn.get_geometry(win)?.reply()?;
    let absolute = conn.translate_coordinates(win, screen_root(conn)?, 0, 0)?.reply()?;
    eprintln!(
        "[{label}] win={win} {}x{} depth={} 绝对位置=({}, {})",
        geo.width, geo.height, geo.depth, absolute.dst_x, absolute.dst_y
    );

    let reply = conn
        .get_image(ImageFormat::Z_PIXMAP, win, 0, 0, geo.width, geo.height, !0)?
        .reply()?;

    let width = geo.width as usize;
    let height = geo.height as usize;
    let stride = reply.data.len() / (width * height);
    eprintln!("[{label}] 数据 {} 字节 stride={stride}", reply.data.len());
    if stride < 3 {
        return Err(format!("意外的像素步长 {stride}").into());
    }

    let mut file = BufWriter::new(File::create(out)?);
    write!(file, "P6\n{width} {height}\n255\n")?;
    // Z_PIXMAP 在 little-endian 上每像素 4 字节：B,G,R,X
    for i in (0..reply.data.len()).step_by(stride) {
        file.write_all(&[reply.data[i + 2], reply.data[i + 1], reply.data[i]])?;
    }
    file.flush()?;

    println!("已写入 {out}（{label} {width}x{height}）");
    Ok(())
}

/// 当前屏幕的 root 窗口。用于把窗口坐标换算成屏幕坐标——注入点击事件用的是屏幕坐标。
fn screen_root(conn: &RustConnection) -> Result<Window, Box<dyn std::error::Error>> {
    Ok(conn.setup().roots[0].root)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/xshot.ppm".to_string());

    let (conn, screen_num) = RustConnection::connect(None)?;
    let screen = &conn.setup().roots[screen_num];
    eprintln!(
        "screen {}x{} root_depth={} root={}",
        screen.width_in_pixels, screen.height_in_pixels, screen.root_depth, screen.root
    );

    let tree = conn.query_tree(screen.root)?.reply()?;
    let mut largest: Option<(Window, u32)> = None;
    for &child in &tree.children {
        let Ok(attrs) = conn.get_window_attributes(child)?.reply() else {
            continue;
        };
        if attrs.map_state != MapState::VIEWABLE {
            continue;
        }
        let Ok(geo) = conn.get_geometry(child)?.reply() else {
            continue;
        };
        let area = u32::from(geo.width) * u32::from(geo.height);
        if largest.is_none_or(|(_, best)| area > best) {
            largest = Some((child, area));
        }
    }

    match largest {
        Some((win, area)) => {
            eprintln!("选中子窗口 {win}（面积 {area}）");
            if let Err(err) = capture(&conn, win, "应用窗口", &out) {
                eprintln!("抓应用窗口失败：{err}，回退到 root");
                capture(&conn, screen.root, "root", &out)?;
            }
        }
        None => {
            eprintln!("没有可映射的子窗口，直接抓 root");
            capture(&conn, screen.root, "root", &out)?;
        }
    }
    Ok(())
}
