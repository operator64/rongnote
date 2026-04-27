//! Serve the SvelteKit SPA build embedded in the binary.
//!
//! Rust-embed bakes everything under `server/static/` into the release binary.
//! At runtime we look up the requested path; if missing, we fall back to
//! `index.html` so client-side routing works.

use axum::{
    body::Body,
    http::{header, HeaderValue, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/static"]
struct Assets;

pub async fn handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.is_empty() {
        return serve("index.html");
    }
    if Assets::get(path).is_some() {
        return serve(path);
    }
    serve("index.html")
}

fn serve(path: &str) -> Response {
    let Some(file) = Assets::get(path) else {
        return placeholder();
    };
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let mut response = Response::new(Body::from(file.data.into_owned()));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(mime.as_ref()).unwrap_or(HeaderValue::from_static("application/octet-stream")),
    );
    // Hashed assets can be cached; HTML must not.
    let cache = if path.ends_with(".html") {
        "no-cache"
    } else {
        "public, max-age=31536000, immutable"
    };
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    response
}

/// Shown when no frontend is bundled. Tells the user to run the dev server.
fn placeholder() -> Response {
    let body = concat!(
        "<!doctype html><meta charset=\"utf-8\"><title>rongnote</title>",
        "<style>body{font:14px ui-monospace,Menlo,monospace;max-width:60ch;",
        "margin:4rem auto;padding:0 1rem;color:#222}code{background:#eee;",
        "padding:0 .25rem}</style>",
        "<h1>rongnote — server is up</h1>",
        "<p>No frontend bundle is embedded in this build.</p>",
        "<p>For dev, run <code>cd web &amp;&amp; npm run dev</code> and visit ",
        "<a href=\"http://localhost:5173\">http://localhost:5173</a>.</p>",
        "<p>API is mounted under <code>/api/v1</code>. Health: ",
        "<a href=\"/healthz\">/healthz</a>.</p>",
    );
    let mut response = Response::new(Body::from(body));
    *response.status_mut() = StatusCode::OK;
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8"));
    response.into_response()
}
