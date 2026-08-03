mod call_graph;
mod call_hierarchy;
mod layout;
mod project;
mod public_view;
mod scip_index;
mod source_flow;
mod syntax;
mod test_view;

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
    call_graph: Arc<call_graph::CallGraph>,
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
    #[serde(default)]
    tests: bool,
}

#[derive(Serialize)]
struct CallsResponse {
    structure: layout::PackedMap,
    source: layout::PackedMap,
    wrap_column: usize,
}

#[derive(Deserialize)]
struct CallsQuery {
    #[serde(default)]
    public: bool,
    #[serde(default)]
    unresolved: bool,
    #[serde(default)]
    tests: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_args()?;
    let root = config.root.canonicalize()?;
    let index_path = root.join("index.scip");
    if !index_path.try_exists()? {
        eprintln!(
            "SCIP index not found at {}.\nBuild it with:\n  {}",
            index_path.display(),
            scip_index::build_command(&root, &index_path)
        );
        return Err("index.scip is required".into());
    }
    let project = scan(&root, config.wrap_column)?;
    if scip_index::is_stale(&root, &project, &index_path)? {
        eprintln!(
            "Warning: source files are newer than {}. Rebuild it with:\n  {}",
            index_path.display(),
            scip_index::build_command(&root, &index_path)
        );
    }
    let semantic = scip_index::load(&index_path, &project)?;
    let call_graph = call_graph::build(&project, &semantic)?;
    let state = AppState {
        project: Arc::new(project),
        call_graph: Arc::new(call_graph),
        wrap_column: config.wrap_column,
    };
    let app = Router::new()
        .route("/api/map", get(get_map))
        .route("/api/calls", get(get_calls))
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
    if !authorized(&headers) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let project = if query.tests {
        state.project.as_ref().clone()
    } else {
        test_view::hide(state.project.as_ref())
    };
    let project = if query.public {
        public_view::build(&project)
    } else {
        project
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

async fn get_calls(
    State(state): State<AppState>,
    Query(query): Query<CallsQuery>,
    headers: HeaderMap,
) -> Result<Json<CallsResponse>, StatusCode> {
    if !authorized(&headers) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let options = call_hierarchy::Options {
        public_only: query.public,
        show_unresolved: query.unresolved,
        inline_source: false,
        show_tests: query.tests,
    };
    let structure = layout::pack(
        call_hierarchy::build(&state.call_graph, options),
        layout::LayoutConfig {
            wrap_column: state.wrap_column,
            mode: layout::LayoutMode::Structure,
        },
    );
    let source = layout::pack(
        call_hierarchy::build(
            &state.call_graph,
            call_hierarchy::Options {
                inline_source: true,
                ..options
            },
        ),
        layout::LayoutConfig {
            wrap_column: state.wrap_column,
            mode: layout::LayoutMode::Source,
        },
    );
    Ok(Json(CallsResponse {
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

fn authorized(headers: &HeaderMap) -> bool {
    headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        == Some(ACCESS_TOKEN)
}

fn local_ip() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((Ipv4Addr::new(192, 0, 2, 1), 80)).ok()?;
    match socket.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(address) => Some(address),
        std::net::IpAddr::V6(_) => None,
    }
}
