use crate::rest::{auth::AuthTokenClaims, ApiResponse, ApiResult};
use rocket::Route;

#[rocket::get("/tunnel/list")]
fn query_user_tunnels(_token: AuthTokenClaims) -> ApiResult {
    ApiResponse::json("TEST")
}

pub fn routes() -> Vec<Route> {
    rocket::routes![query_user_tunnels]
}
