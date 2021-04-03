use rocket::Route;

#[rocket::get("/health")]
async fn get_health() -> &'static str {
    "I am alive and good!"
}

pub fn routes() -> Vec<Route> {
    rocket::routes![get_health]
}
