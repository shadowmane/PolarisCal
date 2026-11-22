//! Calibration certificates: the template, issuing and listing them, and writing the
//! certificate on screen to a PDF with WebView2's own PDF printer, so the file matches
//! the preview exactly.

use std::path::{Path, PathBuf};

use base64::Engine as _;
use polaris_core::certificate::{
    Certificate, CertificateDetails, CertificateFilter, CertificatePaper, CertificateSigner,
    CertificateSummary, CertificateTemplate, MAX_IMAGE_BYTES,
};
use polaris_core::{Error, Id};
use tauri::{AppHandle, State, Webview};
use tauri_plugin_opener::OpenerExt;

use crate::commands::{CmdResult, CommandError, ErrorKind, Need, ctx, require};
use crate::state::AppState;

#[tauri::command(rename_all = "snake_case")]
pub async fn get_certificate_template(
    state: State<'_, AppState>,
) -> CmdResult<CertificateTemplate> {
    let (store, _) = require(&state, Need::SignedIn).await?;
    Ok(store.certificate_template().await?)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn update_certificate_template(
    state: State<'_, AppState>,
    template: CertificateTemplate,
) -> CmdResult<CertificateTemplate> {
    let (store, me) = require(&state, Need::Admin).await?;
    Ok(store
        .update_certificate_template(&ctx(&me, None), template)
        .await?)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_certificates(
    state: State<'_, AppState>,
    filter: CertificateFilter,
) -> CmdResult<Vec<CertificateSummary>> {
    let (store, _) = require(&state, Need::SignedIn).await?;
    Ok(store.list_certificates(&filter).await?)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_certificate(state: State<'_, AppState>, id: Id) -> CmdResult<Certificate> {
    let (store, _) = require(&state, Need::SignedIn).await?;
    Ok(store.get_certificate(id).await?)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn create_certificate(
    state: State<'_, AppState>,
    details: CertificateDetails,
) -> CmdResult<Certificate> {
    let (store, me) = require(&state, Need::Editor).await?;
    Ok(store.create_certificate(&ctx(&me, None), details).await?)
}

/// Active users who can be picked as who calibrated or approved a certificate.
#[tauri::command(rename_all = "snake_case")]
pub async fn list_certificate_signers(
    state: State<'_, AppState>,
) -> CmdResult<Vec<CertificateSigner>> {
    let (store, _) = require(&state, Need::Editor).await?;
    Ok(store.certificate_signers().await?)
}

/// A user's saved signature, for previewing a certificate they sign.
#[tauri::command(rename_all = "snake_case")]
pub async fn get_user_signature(
    state: State<'_, AppState>,
    user_id: Id,
) -> CmdResult<Option<String>> {
    let (store, _) = require(&state, Need::Editor).await?;
    Ok(store.user_signature(user_id).await?)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn set_user_signature(
    state: State<'_, AppState>,
    user_id: Id,
    signature: Option<String>,
) -> CmdResult<()> {
    let (store, me) = require(&state, Need::Admin).await?;
    Ok(store
        .set_user_signature(&ctx(&me, None), user_id, signature)
        .await?)
}

/// Reads an image file (a logo or a signature) and returns it as a data URL.
#[tauri::command(rename_all = "snake_case")]
pub async fn read_image_file(state: State<'_, AppState>, path: String) -> CmdResult<String> {
    require(&state, Need::Admin).await?;
    let path = PathBuf::from(path.trim());
    let mime = match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        Some("svg") => "image/svg+xml",
        _ => {
            return Err(Error::validation(
                "image",
                "Choose a PNG, JPEG, GIF, WebP, BMP, or SVG image.",
            )
            .into());
        }
    };
    let bytes = tauri::async_runtime::spawn_blocking(move || std::fs::read(&path))
        .await
        .map_err(|e| CommandError::new(ErrorKind::Config, e.to_string()))?
        .map_err(|e| {
            CommandError::new(ErrorKind::Config, format!("Couldn't read the image: {e}"))
        })?;
    let url = format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    );
    if url.len() > MAX_IMAGE_BYTES {
        return Err(Error::validation(
            "image",
            "That image is too large. Use one under 1 MB (a few hundred pixels across is plenty).",
        )
        .into());
    }
    Ok(url)
}

/// Writes what this window shows for printing (the certificate) to a PDF at `path`,
/// appends the pages of the `test_data` PDF if one is given, then opens the file in the
/// PDF viewer when `open` is set, ready to print.
#[tauri::command(rename_all = "snake_case")]
pub async fn save_certificate_pdf(
    app: AppHandle,
    state: State<'_, AppState>,
    webview: Webview,
    path: String,
    paper: CertificatePaper,
    test_data: Option<String>,
    open: bool,
) -> CmdResult<()> {
    require(&state, Need::SignedIn).await?;
    let path = PathBuf::from(path.trim());
    let is_pdf = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
    if !path.is_absolute() || !is_pdf {
        return Err(CommandError::new(
            ErrorKind::Validation,
            "Choose where to save the .pdf file.",
        ));
    }
    print_to_pdf(&webview, &path, paper)
        .await
        .map_err(|e| CommandError::new(ErrorKind::Config, format!("Couldn't save the PDF: {e}")))?;
    if let Some(extra) = test_data.map(|p| PathBuf::from(p.trim())) {
        let main = path.clone();
        tauri::async_runtime::spawn_blocking(move || crate::pdf_merge::append_pdf(&main, &extra))
            .await
            .map_err(|e| CommandError::new(ErrorKind::Config, e.to_string()))?
            .map_err(|e| {
                CommandError::new(
                    ErrorKind::Config,
                    format!("The certificate was saved, but the test data wasn't added: {e}"),
                )
            })?;
    }
    if open {
        // The file is saved either way; not opening it isn't worth reporting as a failure.
        let _ = app.opener().open_path(path.to_string_lossy(), None::<&str>);
    }
    Ok(())
}

#[cfg(windows)]
async fn print_to_pdf(
    webview: &Webview,
    path: &Path,
    paper: CertificatePaper,
) -> Result<(), String> {
    use std::sync::mpsc;
    use std::time::Duration;

    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_PRINT_ORIENTATION_PORTRAIT, ICoreWebView2_7, ICoreWebView2Environment6,
    };
    use webview2_com::PrintToPdfCompletedHandler;
    use windows::core::{HSTRING, Interface};

    let (tx, rx) = mpsc::channel::<Result<(), String>>();
    let target = HSTRING::from(path.as_os_str());
    let (width, height) = paper.inches();
    webview
        .with_webview(move |platform| {
            let done = tx.clone();
            let start = move || -> windows::core::Result<()> {
                // SAFETY: WebView2 COM calls on the UI thread, which `with_webview` runs us on.
                unsafe {
                    let core: ICoreWebView2_7 = platform.controller().CoreWebView2()?.cast()?;
                    let env: ICoreWebView2Environment6 = platform.environment().cast()?;
                    let settings = env.CreatePrintSettings()?;
                    settings.SetOrientation(COREWEBVIEW2_PRINT_ORIENTATION_PORTRAIT)?;
                    settings.SetPageWidth(width)?;
                    settings.SetPageHeight(height)?;
                    settings.SetMarginTop(0.0)?;
                    settings.SetMarginBottom(0.0)?;
                    settings.SetMarginLeft(0.0)?;
                    settings.SetMarginRight(0.0)?;
                    settings.SetScaleFactor(1.0)?;
                    settings.SetShouldPrintBackgrounds(true)?;
                    settings.SetShouldPrintHeaderAndFooter(false)?;
                    let handler = PrintToPdfCompletedHandler::create(Box::new(move |result, ok| {
                        let outcome = match (result, ok) {
                            (Ok(()), true) => Ok(()),
                            (Ok(()), false) => {
                                Err("the file couldn't be written. If it's open in another program, close it and try again.".to_string())
                            }
                            (Err(e), _) => Err(e.message().to_string()),
                        };
                        let _ = done.send(outcome);
                        Ok(())
                    }));
                    core.PrintToPdf(&target, &settings, &handler)?;
                }
                Ok(())
            };
            if let Err(e) = start() {
                let _ = tx.send(Err(e.message().to_string()));
            }
        })
        .map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        rx.recv_timeout(Duration::from_secs(60))
            .unwrap_or_else(|_| Err("PDF printing didn't finish.".to_string()))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(not(windows))]
async fn print_to_pdf(_: &Webview, _: &Path, _: CertificatePaper) -> Result<(), String> {
    Err("saving certificates as PDF is only available on Windows.".to_string())
}
