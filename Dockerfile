# The adapter's image: built from this repository, distroless, non-root, with no
# shell and no package manager. Its health check is the binary asking itself.

FROM rust:1.99.1-alpine3.22@sha256:731f64e16c12af70970d9a3655eecb1eecab4898352f09688a8e5ca2ab8b77c5 AS build
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked --bin medusa-adapter && cp target/release/medusa-adapter /medusa-adapter

FROM gcr.io/distroless/static-debian12:nonroot@sha256:afa5c872c891853ca7fcf1f12c3edb23f7eeef36189728842dd51042ff57f7ab
COPY --from=build /medusa-adapter /medusa-adapter
USER nonroot:nonroot
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 CMD ["/medusa-adapter", "health"]
ENTRYPOINT ["/medusa-adapter"]
