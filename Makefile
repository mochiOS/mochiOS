SCRIPTS	= $(shell pwd)/scripts
OUT		= $(shell pwd)/out

.PHONY: all build run clean olddefconfig menuconfig fonts repo-init

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
	@$(SCRIPTS)/build.sh

fonts:
	@$(MAKE) -C libraries/fonts fonts

run: olddefconfig
	@$(SCRIPTS)/runner.sh

clean:
	@rm -rf $(OUT)/*

repo-init:
	@repo init -m default.xml -u $(git rev-parse --show-toplevel) -b $(git rev-parse HEAD)
	@repo sync -j4