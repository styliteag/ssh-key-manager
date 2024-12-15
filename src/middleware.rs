use std::future::{ready, Ready};
use actix_identity::Identity;
use actix_web::{
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    error::ErrorUnauthorized,
    Error, FromRequest,
};
use futures_util::future::LocalBoxFuture;
use std::rc::Rc;

pub struct AuthMiddleware;

impl<S, B> Transform<S, ServiceRequest> for AuthMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = AuthMiddlewareService<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(AuthMiddlewareService {
            service: Rc::new(service),
        }))
    }
}

pub struct AuthMiddlewareService<S> {
    service: Rc<S>,
}

impl<S, B> Service<ServiceRequest> for AuthMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        // Skip authentication for login page, static files, and assets
        if req.path().starts_with("/auth/") || 
           req.path().starts_with("/static/") ||
           req.path().ends_with(".css") ||
           req.path().ends_with(".js") {
            let fut = self.service.call(req);
            return Box::pin(async move { fut.await });
        }

        let (http_req, payload) = req.into_parts();
        let identity = Identity::extract(&http_req);
        let service = self.service.clone();

        Box::pin(async move {
            match identity.await {
                Ok(_) => {
                    let req = ServiceRequest::from_parts(http_req, payload);
                    service.call(req).await
                }
                Err(_) => Err(ErrorUnauthorized("Unauthorized")),
            }
        })
    }
}