HOST ?= mandlerot.local
SSH_USER ?= pi
PI_USER ?= mandlerot
PI_TARGET ?= aarch64-unknown-linux-gnu
INSTALL_DIR ?= /opt/mandlerot
RSYNC_SUDO := --rsync-path="sudo rsync"

# Apple Silicon: cross 0.2.5 image is amd64-only; force Rosetta/qemu emulation.
# Also surface ~/.cargo/bin so rustc/cargo are findable from minimal shells.
ifeq ($(shell uname -s)-$(shell uname -m),Darwin-arm64)
export DOCKER_DEFAULT_PLATFORM := linux/amd64
export PATH := $(HOME)/.cargo/bin:$(PATH)
endif

.PHONY: build build-release build-pi smoke smoke-each deploy logs clean install-pi soak smoke-all

build:
	cargo build

build-release:
	cargo build --release

build-pi:
	@PATH="$(PATH)" cross build --release --target $(PI_TARGET) --no-default-features --features pi

smoke:
	cargo test --test integration_pipeline -- --nocapture

smoke-each:
	@for s in $$(ls scenes/*.glsl | xargs -n1 basename | sed 's/\.glsl$$//'); do \
		echo "smoke: $$s"; \
		cargo run --quiet -- --smoke-frames 30 --config /dev/null 2>&1 | head -3 || true; \
	done

deploy: build-pi
	rsync -avz $(RSYNC_SUDO) target/$(PI_TARGET)/release/mandlerot $(SSH_USER)@$(HOST):$(INSTALL_DIR)/
	rsync -avz $(RSYNC_SUDO) scenes/ $(SSH_USER)@$(HOST):$(INSTALL_DIR)/scenes/
	rsync -avz $(RSYNC_SUDO) config.toml $(SSH_USER)@$(HOST):$(INSTALL_DIR)/

deploy-restart: deploy
	ssh $(SSH_USER)@$(HOST) sudo systemctl restart mandlerot

logs:
	ssh $(SSH_USER)@$(HOST) sudo journalctl -u mandlerot -f

install-pi: build-pi
	rsync -avz deploy/install.sh $(SSH_USER)@$(HOST):/tmp/mandlerot-install.sh
	rsync -avz deploy/mandlerot.service deploy/boot-config-additions.txt $(SSH_USER)@$(HOST):/tmp/
	ssh $(SSH_USER)@$(HOST) sudo bash /tmp/mandlerot-install.sh
	$(MAKE) deploy HOST=$(HOST) SSH_USER=$(SSH_USER)

soak:
	cargo run --release -- --replay tests/fixtures/soak_set.txt --smoke-frames 18000

smoke-all:
	cargo test --test integration_replay
	cargo test --lib

clean:
	cargo clean
