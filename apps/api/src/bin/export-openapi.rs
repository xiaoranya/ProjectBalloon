//! Prints the runtime OpenAPI document as JSON so documentation pipelines
//! (frontend `npm run openapi:gen`) can regenerate TypeScript bindings from
//! the live Rust contract instead of the legacy Java baseline in
//! docs/api/openapi.yaml.

use project_balloon_api::openapi::ApiDoc;
use utoipa::OpenApi;

fn main() {
    let doc = ApiDoc::openapi();
    let json = doc.to_pretty_json().expect("serialize the OpenAPI document");
    println!("{json}");
}
