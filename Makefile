SCRIPTS	= $(shell pwd)/scripts
OUT		= $(shell pwd)/out
MWS		= $(shell pwd)/tools/mws

.PHONY: all build full build-cached run clean olddefconfig menuconfig fonts repo-init install

all: build

olddefconfig:
	@perl $(SCRIPTS)/config/merge-config.pl \
		--default $(shell pwd)/build/defaults.config \
		--in .config \
		--out .config \
		--mk $(shell pwd)/build/config.mk \
		--env $(shell pwd)/build/config.env

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

clean:
	@rm -rf $(OUT)/*

repo-init:
	@repo init -m default.xml -u $(git rev-parse --show-toplevel) -b $(git rev-parse HEAD)
	@repo sync -j4

install:
	@cargo install --path $(MWS) --force
