use actix_web::{
    get, post,
    web::{self, Data, Path},
    Responder,
};
use askama_actix::{Template, TemplateToResponse};

use crate::{
    forms::{FormResponseBuilder, Modal},
    routes::{ErrorTemplate, RenderErrorTemplate},
    sshclient::{HostDiff, SshClient, SshPublicKey},
    ConnectionPool,
};

use crate::models::{Host, User};

pub fn diff_config(cfg: &mut web::ServiceConfig) {
    cfg.service(diff_page)
        .service(render_diff)
        .service(show_diff)
        .service(assign_key_dialog);
}
#[derive(Template)]
#[template(path = "diff/index.html")]
struct DiffPageTemplate {
    hosts: Vec<Host>,
}

#[get("")]
async fn diff_page(conn: Data<ConnectionPool>) -> actix_web::Result<impl Responder> {
    Ok(
        match web::block(move || Host::get_all_hosts(&mut conn.get().unwrap())).await? {
            Ok(hosts) => DiffPageTemplate { hosts }.to_response(),
            Err(error) => ErrorTemplate { error }.to_response(),
        },
    )
}

#[derive(Template)]
#[template(path = "diff/diff.htm")]
struct RenderDiffTemplate {
    diff: HostDiff,
}

#[get("/{host_name}.htm")]
async fn render_diff(
    conn: Data<ConnectionPool>,
    ssh_client: Data<SshClient>,
    host_name: Path<String>,
) -> actix_web::Result<impl Responder> {
    let res = web::block(move || {
        let mut connection = conn.get().unwrap();

        Host::get_host_name(&mut connection, host_name.to_string())
    })
    .await?;

    let host = match res {
        Ok(maybe_host) => {
            let Some(host) = maybe_host else {
                return Ok(RenderErrorTemplate {
                    error: String::from("No such host."),
                }
                .to_response());
            };
            host
        }
        Err(error) => return Ok(RenderErrorTemplate { error }.to_response()),
    };

    let diff = ssh_client.get_host_diff(host).await;

    Ok(RenderDiffTemplate { diff }.to_response())
}

#[derive(Template)]
#[template(path = "diff/show_diff.html")]
struct ShowDiffTemplate {
    host: Host,
}

#[get("/{name}")]
async fn show_diff(
    conn: Data<ConnectionPool>,
    host_name: Path<String>,
) -> actix_web::Result<impl Responder> {
    Ok(
        match web::block(move || {
            Host::get_host_name(&mut conn.get().unwrap(), host_name.to_string())
        })
        .await?
        {
            Ok(host) => {
                let Some(host) = host else {
                    return Ok(ErrorTemplate {
                        error: String::from("Host not found"),
                    }
                    .to_response());
                };
                ShowDiffTemplate { host }.to_response()
            }
            Err(error) => ErrorTemplate { error }.to_response(),
        },
    )
}

#[derive(Template)]
#[template(path = "diff/assign_key_dialog.htm")]
struct AssignKeyDialog {
    key: SshPublicKey,
    users: Vec<User>,
}

#[post("/assign_key_dialog")]
async fn assign_key_dialog(
    conn: Data<ConnectionPool>,
    key: web::Form<SshPublicKey>,
) -> actix_web::Result<impl Responder> {
    let res = web::block(move || User::get_all_users(&mut conn.get().unwrap())).await?;

    Ok(match res {
        Ok(users) => FormResponseBuilder::dialog(Modal {
            title: String::from("Assign this key to a user"),
            request_target: String::from("/users/assign_key"),
            template: AssignKeyDialog { key: key.0, users }.to_string(),
        }),
        Err(error) => FormResponseBuilder::error(error),
    })
}