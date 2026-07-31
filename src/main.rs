mod layout;
mod project;
mod public_view;
mod source_flow;
mod syntax;

use std::{
    env,
    net::{Ipv4Addr, SocketAddr, UdpSocket},
    path::PathBuf,
    sync::Arc,
};

use crate::project::{ProjectNode, scan};
use axum::{
    Json, Router,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    routing::get,
};
use serde::{Deserialize, Serialize};
use tower_http::services::ServeDir;

const ACCESS_TOKEN: &str = "source-map-dev";
const DEFAULT_WRAP_COLUMN: usize = 80;

#[derive(Clone)]
struct AppState {
    project: Arc<ProjectNode>,
    wrap_column: usize,
}

#[derive(Serialize)]
struct MapResponse {
    structure: layout::PackedMap,
    source: layout::PackedMap,
    wrap_column: usize,
}

#[derive(Deserialize)]
struct MapQuery {
    #[serde(default)]
    public: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_args()?;
    let project = scan(&config.root, config.wrap_column)?;
    let state = AppState {
        project: Arc::new(project),
        wrap_column: config.wrap_column,
    };
    let app = Router::new()
        .route("/api/map", get(get_map))
        .fallback_service(ServeDir::new("frontend/dist").append_index_html_on_directories(true))
        .with_state(state.clone());
    let address = SocketAddr::from((Ipv4Addr::UNSPECIFIED, config.port));
    let listener = tokio::net::TcpListener::bind(address).await?;
    let host = local_ip().unwrap_or(Ipv4Addr::LOCALHOST);

    println!(
        "Source map: http://{host}:{}/#token={}",
        config.port, ACCESS_TOKEN
    );
    axum::serve(listener, app).await?;
    Ok(())
}

async fn get_map(
    State(state): State<AppState>,
    Query(query): Query<MapQuery>,
    headers: HeaderMap,
) -> Result<Json<MapResponse>, StatusCode> {
    let supplied = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));

    if supplied != Some(ACCESS_TOKEN) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let project = if query.public {
        public_view::build(state.project.as_ref())
    } else {
        state.project.as_ref().clone()
    };
    let config = layout::LayoutConfig {
        wrap_column: state.wrap_column,
        mode: layout::LayoutMode::Structure,
    };
    let structure = layout::pack(project.clone(), config);
    let source = layout::pack(
        project,
        layout::LayoutConfig {
            mode: layout::LayoutMode::Source,
            ..config
        },
    );
    Ok(Json(MapResponse {
        structure,
        source,
        wrap_column: state.wrap_column,
    }))
}

struct Config {
    root: PathBuf,
    port: u16,
    wrap_column: usize,
}

impl Config {
    fn from_args() -> Result<Self, String> {
        let mut root = None;
        let mut port = 3000;
        let mut wrap_column = DEFAULT_WRAP_COLUMN;
        let mut args = env::args_os().skip(1);

        while let Some(argument) = args.next() {
            match argument.to_str() {
                Some("--port") => {
                    port = parse_next(&mut args, "--port")?;
                }
                Some("--wrap-column") => {
                    wrap_column = parse_next(&mut args, "--wrap-column")?;
                    if wrap_column == 0 {
                        return Err("--wrap-column must be greater than zero".into());
                    }
                }
                Some(value) if value.starts_with('-') => {
                    return Err(format!("unknown option: {value}"));
                }
                _ if root.is_none() => root = Some(PathBuf::from(argument)),
                _ => return Err("only one project path may be supplied".into()),
            }
        }

        Ok(Self {
            root: root.unwrap_or(env::current_dir().map_err(|error| error.to_string())?),
            port,
            wrap_column,
        })
    }
}

fn parse_next<T: std::str::FromStr>(
    args: &mut impl Iterator<Item = std::ffi::OsString>,
    option: &str,
) -> Result<T, String> {
    args.next()
        .and_then(|value| value.into_string().ok())
        .ok_or_else(|| format!("{option} requires a value"))?
        .parse()
        .map_err(|_| format!("invalid value for {option}"))
}

fn local_ip() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((Ipv4Addr::new(192, 0, 2, 1), 80)).ok()?;
    match socket.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(address) => Some(address),
        std::net::IpAddr::V6(_) => None,
    }
}
