SCRIPTS	= $(shell pwd)/scripts
OUT		= $(shell pwd)/out
MWS		= $(shell pwd)/tools/mws
MBOOT_DIR	= $(shell pwd)/mboot
RELEASE_DIR	= $(OUT)/releases
RELEASE_IMAGE	= $(RELEASE_DIR)/mochiOS.img
MBOOT_CONFIG ?= $(MBOOT_DIR)/config/intel-hardware.toml
MBOOT_IMAGE ?= $(OUT)/mochiOS.iso
DRIVER_LINUX_KERNEL ?=
DRIVER_LINUX_INITRAMFS ?=

.PHONY: all build full build-cached hv-device-io-test hv-driver-linux-test hv-image hv-image-test mboot mboot-image mboot-test release run run-boot smoke-log-test smoke-test-kvm smoke-test-tcg tls-http-smoke-test developer-pki-sync-smoke-test developer-pki-production-e2e accounts-https-smoke-test ext2-write-test ext2-write-test-tcg clean clean-runner olddefconfig menuconfig fonts repo-init install

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

hv-image:
	@$(MAKE) -C $(MBOOT_DIR) image \
		CONFIG="$(abspath $(MBOOT_CONFIG))" \
		MNU_DIR="$(CURDIR)/core" \
		IMAGE="$(abspath $(MBOOT_IMAGE))"

hv-image-test:
	@$(MAKE) -C $(MBOOT_DIR) image-test \
		CONFIG="$(abspath $(MBOOT_CONFIG))" \
		MNU_DIR="$(CURDIR)/core" \
		IMAGE="$(abspath $(MBOOT_IMAGE))"

hv-device-io-test:
	@$(MAKE) -C $(MBOOT_DIR) device-io-test \
		MNU_DIR="$(CURDIR)/core"

hv-driver-linux-test:
	@$(MAKE) -C $(MBOOT_DIR) driver-linux-test \
		MNU_DIR="$(CURDIR)/core" \
		DRIVER_LINUX_KERNEL="$(if $(DRIVER_LINUX_KERNEL),$(abspath $(DRIVER_LINUX_KERNEL)))" \
		DRIVER_LINUX_INITRAMFS="$(if $(DRIVER_LINUX_INITRAMFS),$(abspath $(DRIVER_LINUX_INITRAMFS)))"

mboot: mboot-image

mboot-image:
	@test -f $(MBOOT_DIR)/Makefile || { echo "fatal: mBoot repository was not found: $(MBOOT_DIR)" >&2; exit 1; }
	@$(MAKE) -C $(MBOOT_DIR) image \
		CONFIG="$(abspath $(MBOOT_CONFIG))" \
		MNU_DIR="$(CURDIR)/core" \
		IMAGE="$(abspath $(MBOOT_IMAGE))"

mboot-test:
	@$(MAKE) -C $(MBOOT_DIR) test MNU_DIR="$(CURDIR)/core"

release: full mboot-image
	@mkdir -p $(RELEASE_DIR)
	@set -eu; \
		temporary=$(RELEASE_IMAGE).new; \
		rm -f "$$temporary"; \
		cp --sparse=always $(MBOOT_IMAGE) "$$temporary"; \
		chmod 0644 "$$temporary"; \
		mv "$$temporary" $(RELEASE_IMAGE)
	@echo "[done] release image: $(RELEASE_IMAGE)"

fonts:
	@$(MAKE) -C libraries/fonts fonts

run: olddefconfig all
	@$(SCRIPTS)/runner.sh

run-boot:
	@$(MAKE) hv-image-test

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

clean-runner:
	@rm -rf $(OUT)/runner

repo-init:
	@repo init -m default.xml -u $(git rev-parse --show-toplevel) -b $(git rev-parse HEAD)
	@repo sync -j4

install:
	@cargo install --path $(MWS) --force
