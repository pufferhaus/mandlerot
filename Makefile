HOST ?= mandlerot.local
PI_USER ?= mandlerot
PI_TARGET ?= armv7-unknown-linux-gnueabihf
INSTALL_DIR ?= /opt/mandlerot

.PHONY: build build-release build-pi smoke smoke-each deploy logs clean

build:
	cargo build

build-release:
	cargo build --release

build-pi:
	cross build --release --target $(PI_TARGET) --no-default-features --features pi

smoke:
	cargo test --test integration_pipeline -- --nocapture

smoke-each:
	@for s in $$(ls scenes/*.glsl | xargs -n1 basename | sed 's/\.glsl$$//'); do \
		echo "smoke: $$s"; \
		cargo run --quiet -- --smoke-frames 30 --config /dev/null 2>&1 | head -3 || true; \
	done

deploy: build-pi smoke
	rsync -avz target/$(PI_TARGET)/release/mandlerot $(PI_USER)@$(HOST):$(INSTALL_DIR)/
	rsync -avz scenes/ $(PI_USER)@$(HOST):$(INSTALL_DIR)/scenes/
	rsync -avz config.toml $(PI_USER)@$(HOST):$(INSTALL_DIR)/

deploy-restart: deploy
	ssh $(PI_USER)@$(HOST) sudo systemctl restart mandlerot

logs:
	ssh $(PI_USER)@$(HOST) sudo journalctl -u mandlerot -f

clean:
	cargo clean
