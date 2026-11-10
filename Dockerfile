# Build the wasm module when web/pkg is not already in the build context,
# then serve the page with python3 -m http.server 8080.
#
#   docker build -t contraption-web .
#   docker run --rm -p 8080:8080 contraption-web
#
# Open http://localhost:8080
FROM rust:1.99.1-bookworm AS wasm

WORKDIR /src

COPY . .

RUN if [ -f web/pkg/incredible_machine_3d.js ] && [ -f web/pkg/incredible_machine_3d_bg.wasm ]; then \
      echo "web/pkg already exists; skipping the wasm build"; \
    else \
      rustup target add wasm32-unknown-unknown && \
      cargo install wasm-bindgen-cli --version 0.2.130 && \
      cargo build --release --target wasm32-unknown-unknown && \
      wasm-bindgen --out-dir web/pkg --target web \
        target/wasm32-unknown-unknown/release/incredible_machine_3d.wasm; \
    fi

FROM python:3.12-slim

WORKDIR /srv

COPY --from=wasm /src/index.html ./
COPY --from=wasm /src/web/pkg ./web/pkg

EXPOSE 8080

CMD ["python3", "-m", "http.server", "8080"]
