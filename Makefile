SCRIPTS	= $(shell pwd)/scripts
OUT		= $(shell pwd)/out
MWS		= $(shell pwd)/tools/mws
MBOOT_DIR	= $(shell pwd)/mboot
RELEASE_DIR	= $(OUT)/releases
RELEASE_IMAGE	= $(RELEASE_DIR)/mochiOS.img
MBOOT_GUEST_IMAGE	= $(OUT)/artifacts/disk.img
MBOOT_OUTPUT_IMAGE	= $(MBOOT_DIR)/output/images/disk.img

.PHONY: all build full build-cached mboot mboot-image release run run-boot smoke-log-test smoke-test-kvm smoke-test-tcg tls-http-smoke-test developer-pki-sync-smoke-test developer-pki-production-e2e accounts-https-smoke-test ext2-write-test ext2-write-test-tcg clean olddefconfig menuconfig fonts repo-init install

all: build

olddefconfig:
	@perl $(SCRIPTS)/config/merge-config.pl \
		--default $(shell pwd)/build/defaults.config \
		--in .config \
		--out .config \
		--mk $(shell pwd)/build/config.mk

menuconfig:
	@$(MAKE) -C build menuconfig \
		ROOT=$(shell pwd) \
		OUT=$(OUT) \
		SCHEMA=$(shell pwd)/build/schema.conf \
		CONFIG=$(shell pwd)/.config
	@$(MAKE) olddefconfig

build: olddefconfig
	@$(SCRIPTS)/build.sh --cached

full: olddefconfig
	@$(SCRIPTS)/build.sh

build-cached: olddefconfig
	@$(SCRIPTS)/build.sh --cached

mboot: build-cached
	@$(MAKE) mboot-image

mboot-image:
	@test -f $(MBOOT_DIR)/Makefile || { echo "fatal: mBoot repository was not found: $(MBOOT_DIR)" >&2; exit 1; }
	@if [ ! -f $(MBOOT_DIR)/output/.config ]; then \
		$(MAKE) -C $(MBOOT_DIR) defconfig; \
	fi
	@$(MAKE) -C $(MBOOT_DIR) build MOCHIOS=$(MBOOT_GUEST_IMAGE)
	@MBOOT_MOCHIOS_IMAGE=$(MBOOT_GUEST_IMAGE) $(MBOOT_DIR)/scripts/check-image.sh

release: full
	@$(MAKE) mboot-image
	@mkdir -p $(RELEASE_DIR)
	@set -eu; \
		temporary=$(RELEASE_IMAGE).new; \
		rm -f "$$temporary"; \
		cp --sparse=always $(MBOOT_OUTPUT_IMAGE) "$$temporary"; \
		chmod 0644 "$$temporary"; \
		mv "$$temporary" $(RELEASE_IMAGE)
	@echo "[done] release image: $(RELEASE_IMAGE)"

fonts:
	@$(MAKE) -C libraries/fonts fonts

run: olddefconfig all
	@$(SCRIPTS)/runner.sh

run-boot: mboot
	@$(MAKE) -C $(MBOOT_DIR) run-built MOCHIOS=$(MBOOT_GUEST_IMAGE)

smoke-log-test:
	@$(SCRIPTS)/tests/smoke-log-check-test.sh

smoke-test: build smoke-log-test
	@$(SCRIPTS)/smoke-test.sh

smoke-test-kvm: build smoke-log-test
	@QEMU_ACCELERATOR=kvm $(SCRIPTS)/smoke-test.sh

smoke-test-virtio-gpu: build smoke-log-test
	@DEBUG_QEMU_VIRTIO_GPU=y QEMU_ACCELERATOR=kvm $(SCRIPTS)/smoke-test.sh

smoke-test-tcg: build smoke-log-test
	@QEMU_ACCELERATOR=tcg $(SCRIPTS)/smoke-test.sh

tls-http-smoke-test: olddefconfig smoke-log-test
	@$(SCRIPTS)/tls-http-smoke-test.sh

developer-pki-sync-smoke-test:
	@$(SCRIPTS)/developer-pki-sync-smoke-test.sh

developer-pki-production-e2e:
	@$(SCRIPTS)/developer-pki-production-e2e.sh

accounts-https-smoke-test: build smoke-log-test
	@$(SCRIPTS)/accounts-https-smoke-test.sh

ext2-write-test: build
	@QEMU_ACCELERATOR=kvm $(SCRIPTS)/ext2-write-test.sh

ext2-write-test-tcg: build
	@QEMU_ACCELERATOR=tcg $(SCRIPTS)/ext2-write-test.sh

clean:
	@rm -rf $(OUT)/*

repo-init:
	@repo init -m default.xml -u $(git rev-parse --show-toplevel) -b $(git rev-parse HEAD)
	@repo sync -j4

install:
	@cargo install --path $(MWS) --force
