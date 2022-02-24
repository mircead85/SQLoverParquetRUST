#[macro_use] 
extern crate nickel;

use webserver::webserver::start_webserver;

mod dbstore;
mod webserver;
mod shared;
mod test;

fn main() {
    webserver::webserver::print_hello();
    dbstore::dbstore::print_hello();
    //test::test::test_all();

    start_webserver();
}
