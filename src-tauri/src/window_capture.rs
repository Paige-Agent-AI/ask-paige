//! One-shot capture of the main PAIGE WebView. Runs from a blocking worker.
use std::{sync::mpsc, time::{Duration, Instant}};
use tauri::{AppHandle, Manager, webview::PlatformWebview};
use webview2_com::{
    CapturePreviewCompletedHandler,
    Microsoft::Web::WebView2::Win32::COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG,
};
use windows::Win32::{
    System::Com::{IStream, STREAM_SEEK_SET},
    UI::Shell::SHCreateMemStream,
};

use crate::visual_qa::MAX_PNG_BYTES;

pub(crate) fn capture(app: &AppHandle) -> Result<Vec<u8>, String> {
    let window = app.get_webview_window("main").ok_or("The PAIGE window is closed.")?;
    if !window.is_visible().map_err(|e| e.to_string())?
        || window.is_minimized().map_err(|e| e.to_string())?
    {
        return Err("Open the PAIGE Desktop window before capturing it.".into());
    }
    let size = window.inner_size().map_err(|e| e.to_string())?;
    if size.width == 0 || size.height == 0 || size.width > 2560 || size.height > 1600 {
        return Err("Resize the PAIGE window to at most 2560 by 1600 pixels before capturing it.".into());
    }
    let (tx, rx) = mpsc::channel();
    let deadline = Instant::now() + Duration::from_secs(15);
    window.with_webview(move |view| {
        if Instant::now() >= deadline { return; }
        let completed = tx.clone();
        if let Err(error) = start(view, completed) {
            let _ = tx.send(Err(error));
        }
    }).map_err(|_| "Could not reach the PAIGE window.")?;
    rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .map_err(|_| "The PAIGE window capture did not complete.")?
}

fn start(view: PlatformWebview, tx: mpsc::Sender<Result<Vec<u8>, String>>) -> Result<(), String> {
    // Tauri invokes this closure on the WebView's COM thread. The COM objects
    // stay on that thread; only the completed image bytes cross to the worker.
    let core = unsafe { view.controller().CoreWebView2() }
        .map_err(|_| "Could not open PAIGE's WebView for capture.")?;
    let stream = unsafe { SHCreateMemStream(None) }.ok_or("Could not allocate a capture stream.")?;
    let image = stream.clone();
    let completed = CapturePreviewCompletedHandler::create(Box::new(move |status| {
        let result = status.map_err(|_| "WebView2 could not capture PAIGE's window.".into())
            .and_then(|()| read_preview(&image));
        let _ = tx.send(result);
        Ok(())
    }));
    unsafe { core.CapturePreview(COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG, &stream, &completed) }
        .map_err(|_| "Could not start the PAIGE window capture.".into())
}

fn read_preview(stream: &IStream) -> Result<Vec<u8>, String> {
    unsafe { stream.Seek(0, STREAM_SEEK_SET, None) }.map_err(|_| "Could not rewind the capture stream.")?;
    // Read one extra byte so an oversized result is rejected, never truncated.
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 16 * 1024];
    while bytes.len() <= MAX_PNG_BYTES {
        let remaining = (MAX_PNG_BYTES + 1 - bytes.len()).min(chunk.len());
        let mut read = 0;
        unsafe { stream.Read(chunk.as_mut_ptr().cast(), remaining as u32, Some(&mut read)) }
            .ok().map_err(|_| "Could not read the captured PNG.")?;
        if read == 0 {
            return Ok(bytes);
        }
        bytes.extend_from_slice(&chunk[..read as usize]);
    }
    Err("The capture exceeded the PNG size limit. Resize the PAIGE window and try again.".into())
}
