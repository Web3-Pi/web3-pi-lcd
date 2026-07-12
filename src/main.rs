mod app;
mod assets;
mod display;
mod platform;
mod tasks;

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use app::{
    config::{self, EthStatusConfig},
    state::{AppState, InstallStage, SharedState},
};
use display::{
    render::{Renderer, blank_frame},
    st7789::{DisplayBackend, MockDisplay, St7789Display},
};
use image::RgbImage;
use tokio::{signal, sync::RwLock, time};
use tracing::{debug, error, info, warn};

/// Display-supervisor backoff bounds. When the panel can't be brought up (SPI
/// off, nothing wired, GPIO busy) or drops out mid-run, we retry on this
/// schedule instead of exiting — the unit stays `active` and self-heals.
const DISPLAY_RETRY_MIN: Duration = Duration::from_secs(5);
const DISPLAY_RETRY_MAX: Duration = Duration::from_secs(60);

fn init_logging() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,w3p_hwm=debug".into()),
        )
        .init();
}

fn diff_bbox(prev: &RgbImage, curr: &RgbImage) -> Option<(u16, u16, u16, u16)> {
    if prev.dimensions() != curr.dimensions() {
        return None;
    }
    let width = curr.width() as usize;
    let mut min_x = width;
    let mut min_y = curr.height() as usize;
    let mut max_x = 0usize;
    let mut max_y = 0usize;
    let mut changed = false;

    for (idx, (a, b)) in prev.pixels().zip(curr.pixels()).enumerate() {
        if a != b {
            changed = true;
            let x = idx % width;
            let y = idx / width;
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }

    if !changed {
        return None;
    }

    Some((
        min_x as u16,
        min_y as u16,
        (max_x - min_x + 1) as u16,
        (max_y - min_y + 1) as u16,
    ))
}

fn mock_display_requested() -> bool {
    std::env::var("W3P_MOCK_DISPLAY").is_ok_and(|v| v == "1")
}

async fn preflight_checks() -> Result<()> {
    if !platform::checks::is_raspberry_pi() {
        anyhow::bail!("Only Raspberry Pi is supported (set W3P_MOCK_DISPLAY=1 for desktop development)");
    }
    if !platform::checks::is_spi_enabled() && !platform::checks::is_spi_enabled_config() {
        anyhow::bail!(
            "SPI disabled — add 'dtparam=spi=on' to /boot/firmware/config.txt ([all] section) and reboot (see scripts/enable-spi.sh)"
        );
    }
    Ok(())
}

async fn play_animation<D: DisplayBackend + ?Sized>(display: &mut D) -> Result<()> {
    let frames = Renderer::animation_frames();
    if frames.is_empty() {
        warn!("No opening animation frames embedded, skipping animation");
        return Ok(());
    }
    for frame_bytes in frames {
        let frame = Renderer::load_frame(frame_bytes)?;
        if frame.width() != config::GRID_WIDTH || frame.height() != config::GRID_HEIGHT {
            continue;
        }
        display.show_image(&frame)?;
        time::sleep(Renderer::frame_delay()).await;
    }
    time::sleep(time::Duration::from_secs(1)).await;
    Ok(())
}

async fn display_final_screen<D: DisplayBackend + ?Sized>(display: &mut D) {
    match Renderer::final_screen() {
        Ok(img) => {
            if let Err(err) = display.show_image(&img) {
                error!("Display final screen error: {err}");
            }
        }
        Err(err) => error!("Prepare final screen failed: {err}"),
    }
}

async fn render_loop<D: DisplayBackend + ?Sized>(
    display: &mut D,
    renderer: &Renderer,
    state: SharedState,
) -> Result<()> {
    let mut interval = time::interval(time::Duration::from_millis(1000 / config::LOOP_FPS));
    interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
    let mut install_interval = time::interval(time::Duration::from_millis(1000 / config::INSTALL_FPS));
    install_interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

    let mut cached_dashboard = blank_frame();
    let mut cached_tick = 0_u64;
    let mut blink = false;
    let animation_started_at = Instant::now();
    let mut prev_dashboard_frame: Option<RgbImage> = None;
    loop {
        let snapshot = { state.read().await.clone() };
        if snapshot.install.stage != InstallStage::Done {
            install_interval.tick().await;
            let img = renderer.render_install_screen(&snapshot, blink);
            blink = !blink;
            display.show_image(&img)?;
            let mut guard = state.write().await;
            guard.advance_spinner();
            guard.ui.animation_tick = guard.ui.animation_tick.wrapping_add(1);
            prev_dashboard_frame = None;
            continue;
        }

        if snapshot.install.needs_stage_done_animation {
            play_animation(display).await?;
            let mut guard = state.write().await;
            guard.install.needs_stage_done_animation = false;
            prev_dashboard_frame = None;
        }

        interval.tick().await;
        if cached_tick.is_multiple_of(config::LOOP_FPS) {
            cached_dashboard = renderer.render_dashboard_base(&snapshot);
        }

        let mut frame = cached_dashboard.clone();
        renderer.draw_dashboard_animation(&mut frame, animation_started_at.elapsed().as_secs_f32());

        match prev_dashboard_frame.as_ref().and_then(|prev| diff_bbox(prev, &frame)) {
            Some((x, y, width, height)) => {
                let ts = Instant::now();
                display.show_region(&frame, x, y, width, height)?;
                debug!(
                    x,
                    y,
                    width,
                    height,
                    elapsed = ?ts.elapsed(),
                    "show region"
                );
            }
            None if prev_dashboard_frame.is_some() => {}
            None => {
                let ts = Instant::now();
                display.show_image(&frame)?;
                debug!(elapsed = ?ts.elapsed(), "show image");
            }
        }
        prev_dashboard_frame = Some(frame);

        cached_tick = cached_tick.wrapping_add(1);
        let mut guard = state.write().await;
        guard.ui.animation_tick = guard.ui.animation_tick.wrapping_add(1);
    }
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut term = signal::unix::signal(signal::unix::SignalKind::terminate()).ok();
        tokio::select! {
            _ = signal::ctrl_c() => {},
            _ = async {
                if let Some(t) = term.as_mut() {
                    t.recv().await;
                } else {
                    std::future::pending::<()>().await;
                }
            } => {},
        }
    }
    #[cfg(not(unix))]
    {
        let _ = signal::ctrl_c().await;
    }
}

/// Bring the panel up and drive the render loop until a shutdown signal or a
/// display failure. Returns `Ok(())` on a clean shutdown and `Err` if the panel
/// could not be initialised or dropped out mid-run — the supervisor decides
/// whether to retry.
async fn run_display_session(mock: bool, renderer: &Renderer, state: SharedState) -> Result<()> {
    if !mock {
        preflight_checks().await?;
    }

    let mut display: Box<dyn DisplayBackend> = if mock {
        Box::new(MockDisplay::new())
    } else {
        Box::new(St7789Display::new().context("hardware display init")?)
    };

    display.init().context("display init")?;
    display.clear().context("display clear")?;
    display.set_backlight(100).context("set backlight")?;

    if Renderer::opening_needed() {
        if let Err(err) = play_animation(display.as_mut()).await {
            warn!("Opening animation failed: {err}");
        }
    } else {
        Renderer::create_opening_flag();
    }

    {
        let render_future = render_loop(display.as_mut(), renderer, state);
        tokio::pin!(render_future);
        tokio::select! {
            _ = shutdown_signal() => info!("Shutdown signal received"),
            res = &mut render_future => {
                res.context("render loop")?;
                info!("Render loop exited");
            }
        }
    }

    // Best-effort dimmed logo on the way out; harmless no-op if the panel is gone.
    display_final_screen(display.as_mut()).await;
    Ok(())
}

/// Spawn the background collectors. They never touch the panel, so they live for
/// the whole process and log (rather than propagate) failures — a transient
/// collector error must not tear the dashboard down.
fn spawn_background_tasks(state: &SharedState) {
    let eth_cfg = EthStatusConfig::from_env();
    spawn_supervised(
        "system_metrics_high",
        tasks::system_metrics::high_frequency_loop(state.clone()),
    );
    spawn_supervised(
        "system_metrics_medium",
        tasks::system_metrics::medium_frequency_loop(state.clone()),
    );
    spawn_supervised(
        "system_metrics_low",
        tasks::system_metrics::low_frequency_loop(state.clone()),
    );
    spawn_supervised(
        "install_stage",
        tasks::install_stage::install_stage_loop(state.clone()),
    );
    spawn_supervised(
        "eth_status",
        tasks::eth_status::eth_status_loop(state.clone(), eth_cfg),
    );
}

fn spawn_supervised<F>(name: &'static str, fut: F)
where
    F: std::future::Future<Output = Result<()>> + Send + 'static,
{
    tokio::spawn(async move {
        match fut.await {
            Ok(()) => info!(task = name, "Background task ended"),
            Err(err) => error!(task = name, "Background task error: {err:#}"),
        }
    });
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    init_logging();
    info!("Hardware Monitor Start");
    info!(
        status_path = %config::install_status_path(),
        vos_stage_path = config::VOS_INSTALL_STAGE_PATH,
        install_grace_seconds = config::install_startup_grace().as_secs(),
        install_warn_rate_limit_seconds = config::INSTALL_WARN_RATE_LIMIT.as_secs(),
        rust_log = std::env::var("RUST_LOG").unwrap_or_else(|_| "<not-set>".to_owned()),
        "Runtime diagnostics"
    );

    let state: SharedState = Arc::new(RwLock::new(AppState::default()));
    let renderer = Renderer::load().context("load embedded renderer assets")?;

    // Collectors run regardless of panel state so telemetry is fresh the moment
    // a display appears.
    spawn_background_tasks(&state);

    let mock = mock_display_requested();
    if mock {
        info!("W3P_MOCK_DISPLAY=1 set, using mock display");
    }

    // Display supervisor: a missing or failing panel is NOT fatal. Retry with
    // capped backoff so the unit stays `active` and self-heals when SPI is
    // enabled / the panel is wired / a transient SPI error clears. Only a
    // shutdown signal ends the process.
    let mut backoff = DISPLAY_RETRY_MIN;
    loop {
        match run_display_session(mock, &renderer, state.clone()).await {
            Ok(()) => break,
            Err(err) => warn!(retry_in_seconds = backoff.as_secs(), "Display unavailable: {err:#}"),
        }
        tokio::select! {
            _ = shutdown_signal() => break,
            _ = time::sleep(backoff) => {}
        }
        backoff = backoff.saturating_mul(2).min(DISPLAY_RETRY_MAX);
    }

    info!("Hardware Monitor End");
    Ok(())
}

#[cfg(test)]
mod tests {
    use image::{ImageBuffer, Rgb};

    use super::diff_bbox;

    #[test]
    fn diff_bbox_none_when_equal() {
        let a = ImageBuffer::from_pixel(4, 3, Rgb([0, 0, 0]));
        let b = ImageBuffer::from_pixel(4, 3, Rgb([0, 0, 0]));
        assert_eq!(diff_bbox(&a, &b), None);
    }

    #[test]
    fn diff_bbox_single_pixel() {
        let a = ImageBuffer::from_pixel(4, 3, Rgb([0, 0, 0]));
        let mut b = ImageBuffer::from_pixel(4, 3, Rgb([0, 0, 0]));
        b.put_pixel(2, 1, Rgb([1, 2, 3]));
        assert_eq!(diff_bbox(&a, &b), Some((2, 1, 1, 1)));
    }

    #[test]
    fn diff_bbox_multi_pixel_bounds() {
        let a = ImageBuffer::from_pixel(6, 5, Rgb([0, 0, 0]));
        let mut b = ImageBuffer::from_pixel(6, 5, Rgb([0, 0, 0]));
        b.put_pixel(1, 2, Rgb([1, 2, 3]));
        b.put_pixel(4, 4, Rgb([4, 5, 6]));
        assert_eq!(diff_bbox(&a, &b), Some((1, 2, 4, 3)));
    }
}
