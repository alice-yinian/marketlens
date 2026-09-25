// Windows 发布版不弹出附带的控制台窗口
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    marketlens_lib::run();
}
