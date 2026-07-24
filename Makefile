SCRIPTS	= $(shell pwd)/scripts
OUT		= $(shell pwd)/out
MWS		= $(shell pwd)/tools/mws

.PHONY: all build full build-cached run smoke-log-test smoke-test smoke-test-kvm smoke-test-tcg ext2-write-test ext2-write-test-tcg clean olddefconfig menuconfig fonts repo-init install

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

fonts:
	@$(MAKE) -C libraries/fonts fonts

run: olddefconfig all
	@$(SCRIPTS)/runner.sh

smoke-log-test:
	@$(SCRIPTS)/tests/smoke-log-check-test.sh

smoke-test: build smoke-log-test
	@$(SCRIPTS)/smoke-test.sh

smoke-test-kvm: build smoke-log-test
	@QEMU_ACCELERATOR=kvm $(SCRIPTS)/smoke-test.sh

smoke-test-tcg: build smoke-log-test
	@QEMU_ACCELERATOR=tcg $(SCRIPTS)/smoke-test.sh

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
