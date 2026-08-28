SCRIPTS	= $(shell pwd)/scripts
OUT		= $(shell pwd)/out
MWS		= $(shell pwd)/tools/mws
MBOOT_DIR	= $(shell pwd)/mboot
RELEASE_DIR	= $(OUT)/releases
RELEASE_IMAGE	= $(RELEASE_DIR)/mochiOS.img
MBOOT_CONFIG ?= $(MBOOT_DIR)/config/intel-hardware.toml
MBOOT_IMAGE ?= $(OUT)/mochiOS.img
MBOOT_PXE_DIR ?= $(OUT)/pxe
MDRIVER_KERNEL ?= $(MBOOT_DIR)/mdriver/output/artifacts/vmlinux
MDRIVER_INITRAMFS ?= $(MBOOT_DIR)/mdriver/output/artifacts/initramfs.cpio

.PHONY: all build full build-cached hv-device-io-test hv-image hv-image-test image mboot mboot-image mboot-setup mboot-test mdriver mdriver-test release run run-boot smoke-log-test smoke-test-kvm smoke-test-tcg storage-probe-image tls-http-smoke-test developer-pki-sync-smoke-test developer-pki-production-e2e accounts-https-smoke-test ext2-write-test ext2-write-test-tcg clean clean-runner olddefconfig menuconfig fonts repo-init install

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

hv-image: mdriver
	@$(MAKE) -C $(MBOOT_DIR) image \
		CONFIG="$(abspath $(MBOOT_CONFIG))" \
		MNU_DIR="$(CURDIR)/core" \
		IMAGE="$(abspath $(MBOOT_IMAGE))" \
		PXE_DIR="$(abspath $(MBOOT_PXE_DIR))" \
		MDRIVER_KERNEL="$(abspath $(MDRIVER_KERNEL))" \
		MDRIVER_INITRAMFS="$(abspath $(MDRIVER_INITRAMFS))"

hv-image-test: mdriver
	@$(MAKE) -C $(MBOOT_DIR) image-test \
		CONFIG="$(abspath $(MBOOT_CONFIG))" \
		MNU_DIR="$(CURDIR)/core" \
		IMAGE="$(abspath $(MBOOT_IMAGE))" \
		PXE_DIR="$(abspath $(MBOOT_PXE_DIR))" \
		MDRIVER_KERNEL="$(abspath $(MDRIVER_KERNEL))" \
		MDRIVER_INITRAMFS="$(abspath $(MDRIVER_INITRAMFS))"

hv-device-io-test:
	@$(MAKE) -C $(MBOOT_DIR) device-io-test \
		MNU_DIR="$(CURDIR)/core"

mdriver:
	@$(MAKE) -C $(MBOOT_DIR)/mdriver build

mdriver-test: mdriver
	@$(MAKE) -C $(MBOOT_DIR) mdriver-test \
		MNU_DIR="$(CURDIR)/core" \
		MDRIVER_KERNEL="$(abspath $(MDRIVER_KERNEL))" \
		MDRIVER_INITRAMFS="$(abspath $(MDRIVER_INITRAMFS))"

mboot: mboot-image

image: mboot-image

storage-probe-image:
	@$(MAKE) mboot-image \
		MBOOT_CONFIG="$(MBOOT_DIR)/config/intel-storage-probe.toml" \
		MBOOT_IMAGE="$(OUT)/mochiOS-storage-probe.img" \
		MBOOT_PXE_DIR="$(OUT)/pxe-storage-probe"

mboot-setup:
	@MNU_DIR="$(CURDIR)/core" $(MBOOT_DIR)/setup.sh

mboot-image: mdriver
	@test -f $(MBOOT_DIR)/Makefile || { echo "fatal: mBoot repository was not found: $(MBOOT_DIR)" >&2; exit 1; }
	@$(MAKE) -C $(MBOOT_DIR) image \
		CONFIG="$(abspath $(MBOOT_CONFIG))" \
		MNU_DIR="$(CURDIR)/core" \
		IMAGE="$(abspath $(MBOOT_IMAGE))" \
		PXE_DIR="$(abspath $(MBOOT_PXE_DIR))" \
		MDRIVER_KERNEL="$(abspath $(MDRIVER_KERNEL))" \
		MDRIVER_INITRAMFS="$(abspath $(MDRIVER_INITRAMFS))"

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
	@$(MAKE) -C $(MBOOT_DIR) clean

clean-runner:
	@rm -rf $(OUT)/runner

repo-init:
	@repo init -m default.xml -u $(git rev-parse --show-toplevel) -b $(git rev-parse HEAD)
	@repo sync -j4

install:
	@cargo install --path $(MWS) --force
