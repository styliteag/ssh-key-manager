use actix_web::{
    get,
    post,
    web::{self, Data, Path}, Responder,
};
use askama_actix::{Template, TemplateToResponse};
use log::debug;
use serde::Deserialize;

use crate::{
    db::{UserAndOptions},
    forms::{FormResponseBuilder, Modal},
    routes::RenderErrorTemplate,
    sshclient::{ConnectionDetails, SshClient},
    ConnectionPool, DbConnection,
};

use crate::models::{Host, NewHost, User};

pub fn hosts_config(cfg: &mut web::ServiceConfig) {
    cfg.service(hosts_page)
        .service(render_hosts)
        .service(show_host)
        .service(add_host)
        .service(remove_key_from_host)
        .service(authorize_user);
}

#[derive(Deserialize)]
struct RemoveKeyFromHostForm {
    key_base64: String,
}

#[post("/{name}/remove_key")]
async fn remove_key_from_host(
    conn: Data<ConnectionPool>,
    ssh_client: Data<SshClient>,
    host_name: Path<String>,
    key: web::Form<RemoveKeyFromHostForm>,
) -> actix_web::Result<impl Responder> {
    let res = ssh_client
        .remove_key(host_name.to_string(), key.0.key_base64)
        .await;

    Ok(match res {
        Ok(()) => FormResponseBuilder::success(String::from("Removed key from host")),
        Err(e) => FormResponseBuilder::error(e.to_string()),
    })
}

#[derive(Template)]
#[template(path = "hosts/index.html")]
struct HostsTemplate {}

#[get("")]
async fn hosts_page() -> impl Responder {
    HostsTemplate {}
}

#[derive(Template)]
#[template(path = "hosts/show_host.html")]
struct ShowHostTemplate {
    host: Host,
    jumphost: Option<String>,
    authorized_users: Vec<UserAndOptions>,
    users: Vec<User>,
}

type HostData = (Host, Option<String>, Vec<UserAndOptions>, Vec<User>);

enum HostDataError {
    HostNotFound,
    DatabaseError(String),
}

fn get_all_host_data(conn: &mut DbConnection, host: String) -> Result<HostData, HostDataError> {
    let maybe_host = Host::get_host_name(conn, host).map_err(HostDataError::DatabaseError)?;

    let Some(host) = maybe_host else {
        return Err(HostDataError::HostNotFound);
    };

    let jumphost = if let Some(id) = host.jump_via {
        Host::get_host_id(conn, id)
            .map_err(HostDataError::DatabaseError)?
            .map(|h| h.name)
    } else {
        None
    };

    let authorized_users = host
        .get_authorized_users(conn)
        .map_err(HostDataError::DatabaseError)?;

    let user_list = User::get_all_users(conn).map_err(HostDataError::DatabaseError)?;

    Ok((host, jumphost, authorized_users, user_list))
}

#[get("/{name}")]
async fn show_host(
    conn: Data<ConnectionPool>,
    host: Path<String>,
) -> actix_web::Result<impl Responder> {
    let res =
        web::block(move || get_all_host_data(&mut conn.get().unwrap(), host.to_string())).await?;

    let (host, jumphost, authorized_users, user_list) = match res {
        Ok(host_data) => host_data,
        Err(e) => {
            return Ok(match e {
                HostDataError::HostNotFound => {
                    FormResponseBuilder::not_found(String::from("Host not found")).into_response()
                }
                HostDataError::DatabaseError(e) => FormResponseBuilder::error(e).into_response(),
            })
        }
    };

    Ok(ShowHostTemplate {
        host,
        jumphost,
        authorized_users,
        users: user_list,
    }
    .to_response())
}

#[derive(Template)]
#[template(path = "hosts/hostkey_dialog.htm")]
struct HostkeyDialog {
    name: String,
    username: String,
    hostname: String,
    port: i32,
    key_fingerprint: String,
    jumphost: Option<i32>,
}

#[derive(Deserialize)]
struct HostAddForm {
    name: String,
    username: String,
    hostname: String,
    port: i32,
    jumphost: Option<i32>,
    key_fingerprint: Option<String>,
}

#[post("/add")]
async fn add_host(
    conn: Data<ConnectionPool>,
    ssh_client: Data<SshClient>,
    form: web::Form<HostAddForm>,
) -> actix_web::Result<impl Responder> {
    let form = form.0;

    // TODO: better error handling for jumphost (serde deserialize opt)
    let cloned_conn = conn.clone();
    let maybe_jumphost: Option<Host> = if let Some(via) = form.jumphost {
        if via < 0 {
            None
        } else {
            match web::block(move || Host::get_host_id(&mut cloned_conn.get().unwrap(), via))
                .await?
            {
                Ok(j) => j,
                Err(_) => {
                    return Ok(FormResponseBuilder::not_found(String::from(
                        "Couldn't find jump host",
                    )));
                }
            }
        }
    } else {
        None
    };
    let Ok(address) = ConnectionDetails::new_from_signed(form.hostname.clone(), form.port) else {
        return Ok(FormResponseBuilder::error(String::from(
            "Invalid port number",
        )));
    };
    debug!(
        "Trying to connect to {} on port {} via jumphost: {:?}",
        &address.hostname, &address.port, maybe_jumphost
    );
    let Some(key_fingerprint) = form.key_fingerprint else {
        let connection_res = match maybe_jumphost {
            Some(via) => ssh_client.get_hostkey_via(via, address).await,
            None => ssh_client.get_hostkey(address).await,
        };

        let key_receiver = match connection_res {
            Ok(r) => r,
            Err(e) => return Ok(FormResponseBuilder::error(e.to_string())),
        };

        let Ok(key_fingerprint) = web::block(move || key_receiver.recv()).await? else {
            return Ok(FormResponseBuilder::error(String::from(
                "Connection timed out",
            )));
        };

        return Ok(FormResponseBuilder::dialog(Modal {
            title: String::from("Please check the hostkey"),
            request_target: String::from("/hosts/add"),
            template: HostkeyDialog {
                name: form.name,
                username: form.username,
                hostname: form.hostname,
                port: form.port,
                jumphost: form.jumphost,
                key_fingerprint,
            }
            .to_string(),
        }));
    };

    if let Err(error) = {
        match maybe_jumphost {
            Some(ref via) => {
                ssh_client
                    .try_authenticate_via(
                        via.clone(),
                        address,
                        key_fingerprint.clone(),
                        form.username.clone(),
                    )
                    .await
            }
            None => {
                ssh_client
                    .try_authenticate(address, key_fingerprint.clone(), form.username.clone())
                    .await
            }
        }
    } {
        return Ok(FormResponseBuilder::error(error.to_string()));
    };

    let new_host = NewHost {
        name: form.name.clone(),
        hostname: form.hostname,
        port: form.port,
        username: form.username,
        key_fingerprint,
        jump_via: maybe_jumphost.map(|h| Some(h.id)).unwrap_or(None),
    };
    let res = web::block(move || Host::add_host(&mut conn.get().unwrap(), &new_host)).await?;

    Ok(match res {
        Ok(()) => FormResponseBuilder::created(String::from("Added host"))
            .add_trigger(String::from("reload-hosts")),
        Err(e) => FormResponseBuilder::error(e),
    })
}

#[derive(Template)]
#[template(path = "hosts/list.htm")]
struct RenderHostsTemplate {
    hosts: Vec<Host>,
}

#[get("/list.htm")]
async fn render_hosts(conn: Data<ConnectionPool>) -> actix_web::Result<impl Responder> {
    let all_hosts = web::block(move || Host::get_all_hosts(&mut conn.get().unwrap())).await?;

    Ok(match all_hosts {
        Ok(all_hosts) => RenderHostsTemplate { hosts: all_hosts }.to_response(),
        Err(error) => RenderErrorTemplate { error }.to_response(),
    })
}
#[derive(Deserialize)]
struct AuthorizeUserForm {
    host_id: i32,
    user_id: i32,
    options: Option<String>,
}

#[post("/user/authorize")]
async fn authorize_user(
    conn: Data<ConnectionPool>,

    form: web::Form<AuthorizeUserForm>,
) -> actix_web::Result<impl Responder> {
    let res = web::block(move || {
        Host::authorize_user(
            &mut conn.get().unwrap(),
            form.host_id,
            form.user_id,
            form.options.clone(),
        )
    })
    .await?;

    Ok(match res {
        Ok(()) => FormResponseBuilder::success(String::from("Authorized user")),
        Err(e) => FormResponseBuilder::error(e),
    })
}