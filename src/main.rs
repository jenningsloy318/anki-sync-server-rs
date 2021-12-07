#![forbid(unsafe_code)]
mod db;
mod media;
pub mod parse;
pub mod session;
pub mod sync;
pub mod user;
use self::{
    session::SessionManager,
    sync::{favicon, sync_app, welcome},
    user::{create_auth_db, user_manage},
};
use actix_web::{middleware, web, App, HttpServer};
use anki::{backend::Backend, i18n::I18n};
use parse::{
    conf::{create_conf, LocalCert, Settings},
    parse,
};
use rustls::internal::pemfile::{certs, pkcs8_private_keys};
use rustls::{NoClientAuth, ServerConfig};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Mutex;
use user::create_account;

/// "cert.pem" "key.pem"
fn load_ssl(localcert: LocalCert) -> Option<ServerConfig> {
    // load ssl keys
    let enable = localcert.ssl_enable;
    if enable {
        let cert = localcert.cert_file;
        let key = localcert.key_file;

        let mut config = ServerConfig::new(NoClientAuth::new());
        let cert_file = &mut BufReader::new(File::open(cert).unwrap());
        let key_file = &mut BufReader::new(File::open(key).unwrap());
        let cert_chain = certs(cert_file).unwrap();
        let mut keys = pkcs8_private_keys(key_file).unwrap();
        if keys.is_empty() {
            eprintln!("Could not locate PKCS 8 private keys.");
            std::process::exit(1);
        }
        config.set_single_cert(cert_chain, keys.remove(0)).unwrap();
        Some(config)
    } else {
        None
    }
}
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    //cli argument  parse
    let matches = parse();
    // set config path if parsed and write conf settings
    // to path
    let conf_path = Path::new(matches.value_of("config").unwrap());
    create_conf(&conf_path);
    // read config file
    let conf = Settings::new().unwrap();

    // create db if not exist
    let auth_path = Path::new(&conf.paths.root_dir).join("auth.db");
    create_auth_db(&auth_path).unwrap();
    // enter into account manage if subcommand exists,else run server
    if matches.subcommand_name().is_some() {
        user_manage(matches, auth_path);
        Ok(())
    } else {
        //    run ankisyncd without any sub-command

        create_account(conf.account, auth_path);
        let ssl_config = load_ssl(conf.localcert);
        // parse ip address
        let addr = format!("{}:{}", conf.address.host, conf.address.port);

        std::env::set_var("RUST_LOG", "actix_server=info,actix_web=info");
        env_logger::init();
        let session_manager = web::Data::new(Mutex::new(SessionManager::new()));
        let tr = I18n::template_only();
        let bd = web::Data::new(Mutex::new(Backend::new(tr, true)));
        let s = HttpServer::new(move || {
            App::new()
                .app_data(session_manager.clone())
                .app_data(bd.clone())
                .service(welcome)
                .service(favicon)
                .service(web::resource("/{url}/{name}").to(sync_app))
                .wrap(middleware::Logger::default())
        });
        if let Some(c) = ssl_config {
            s.bind_rustls(addr, c)?.run().await
        } else {
            s.bind(addr)?.run().await
        }
    }
}
