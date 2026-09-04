use gpui::{AppContext, Application};
use scribe::app::{self, AppModel};
use scribe::config::Config;
use scribe::hotkey;
use scribe::paths::AppPaths;
use scribe::pipeline::Session;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "scribe=info,gpui=warn".parse().expect("filter")),
        )
        .init();

    let paths = match AppPaths::new() {
        Ok(paths) => paths,
        Err(err) => {
            eprintln!("scribe: {err:#}");
            std::process::exit(1);
        }
    };
    if let Err(err) = paths.ensure() {
        eprintln!("scribe: {err:#}");
        std::process::exit(1);
    }
    let config = Config::load(&paths.config_file()).unwrap_or_default();
    let _ = config.save(&paths.config_file());

    Application::new().run(move |cx| {
        let (rx, evdev) = match hotkey::start_listeners(
            config.hotkeys.toggle.clone(),
            config.hotkeys.push_to_talk.clone(),
            config.hotkeys.cancel.clone(),
        ) {
            Ok(pair) => pair,
            Err(err) => {
                tracing::error!("hotkeys: {err:#}");
                let (tx, rx) = flume::unbounded();
                drop(tx);
                (rx, None)
            }
        };
        let session = Session::new(paths.clone(), config.clone());
        let model = cx.new(|_| AppModel::new(session, evdev));
        app::open_settings(cx, model.clone());
        app::wire_hotkeys(model, rx, cx);
        cx.activate(true);
    });
}
