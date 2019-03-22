// #![deny(warnings)]
extern crate hyper;
extern crate pretty_env_logger;
#[macro_use]
extern crate log;

use std::env;
// use std::io::{self, Write};

use hyper::Client;
use hyper::rt::{self, Future, Stream};

fn main() {
    pretty_env_logger::init();

    // Some simple CLI args requirements...
    let url = match env::args().nth(1) {
        Some(url) => url,
        None => {
            "http://httpbin.org/post".to_owned()
            // println!("Usage: client <url>");
            // return;
        }
    };

    // HTTPS requires picking a TLS implementation, so give a better
    // warning if the user tries to request an 'https' URL.
    let url = url.parse::<hyper::Uri>().unwrap();
    if url.scheme_part().map(|s| s.as_ref()) != Some("http") {
        println!("This example only works with 'http' URLs.");
        return;
    }

    // Run the runtime with the future trying to fetch and print this URL.
    //
    // Note that in more complicated use cases, the runtime should probably
    // run on its own, and futures should just be spawned into it.
    rt::run(fetch_url(url));
}

fn fetch_url(url: hyper::Uri) -> impl Future<Item=(), Error=()> {
    use hyper::{Body, Method, Request};
    let client = Client::new();

    let json = r#"{"library":"hyper"}"#;
    let mut req = Request::new(Body::from(json));
    *req.method_mut() = Method::POST;
    *req.uri_mut() = url;
    req.headers_mut().insert(hyper::header::CONTENT_TYPE, hyper::header::HeaderValue::from_static("application/json"));

    debug!("send request thread: {:?}", std::thread::current());
    client
        .request(req)
        // And then, if we get a response back...
        .and_then(|res| {
            debug!("Response: {}", res.status());
            debug!("Headers: {:#?}", res.headers());
            debug!("header thread: {:?}", std::thread::current());

            // The body is a stream, and for_each returns a new Future
            // when the stream is finished, and calls the closure on
            // each chunk of the body...
            res.into_body().concat2()
            // res.into_body().for_each(|chunk| {
            //     io::stdout().write_all(&chunk)
            //         .map_err(|e| panic!("example expects stdout is open, error={}", e))
            // })
        })
        // If all good, just tell the user...
        .map(|body| {
            if let Some(t_info) = hyper::client::get_transport_info() {
                debug!("body thread: {:?}", std::thread::current());
                debug!("raw fd: {:?}", hyper::client::get_connection_fd());

                debug!("transport info: {:?}", t_info);
                let hyper::TransportInfo {
                    req_finished_ts, res_begin_ts, res_header_finished_ts, res_header_length
                } = t_info;

                let req_finished = req_finished_ts.unwrap();
                let res_begin = res_begin_ts.unwrap();
                let res_header_finished = res_header_finished_ts.unwrap();
                debug!("req finished to res begin: {:?}", res_begin - req_finished);
                debug!("res begin to res header finished: {:?}", res_header_finished - res_begin);
                debug!("res header len: {:?}", res_header_length);
            }

            debug!("Done. Body: {:?}", body);
        })
        // If there was an error, let the user know...
        .map_err(|err| {
            error!("Error {}", err);
        })
}
