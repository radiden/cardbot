FROM messense/rust-musl-cross:x86_64-musl as builder

COPY ./ /home/rust/src

RUN cargo build --release --target x86_64-unknown-linux-musl

FROM alpine:latest as runner

RUN mkdir /app
COPY --from=builder /home/rust/src/target/x86_64-unknown-linux-musl/release/cardbot /app
COPY ./migrations /app
CMD ["/app/cardbot"]