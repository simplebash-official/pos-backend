/// Browser origins a single-shop backend answers when `CORS_ALLOWED_ORIGINS`
/// is unset: the Tauri desktop webview (macOS/Linux `tauri://localhost`,
/// Windows `http(s)://tauri.localhost`) and the Vite dev server. A web
/// deployment whose UI is served from another origin must list it in
/// `CORS_ALLOWED_ORIGINS`.
pub const DEFAULT_SINGLE_SHOP_ORIGINS: &[&str] = &[
    "tauri://localhost",
    "http://tauri.localhost",
    "https://tauri.localhost",
    "http://localhost:5173",
    "http://127.0.0.1:5173",
];
