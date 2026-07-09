SCRIPTS	= $(shell pwd)/scripts
OUT		= $(shell pwd)/out

.PHONY: all build run clean olddefconfig menuconfig repo-init

all: build

olddefconfig:
	@perl $(SCRIPTS)/config/merge-config.pl \
		--default $(SCRIPTS)/config/defaults.config \
		--in .config \
		--out .config \
		--mk $(SCRIPTS)/config/config.mk \
		--env $(SCRIPTS)/config/config.env

menuconfig:
	@$(MAKE) -C build menuconfig \
		ROOT=$(shell pwd) \
		OUT=$(OUT) \
		SCHEMA=$(SCRIPTS)/config/schema.conf \
		CONFIG=$(shell pwd)/.config
	@$(MAKE) olddefconfig

build: olddefconfig
	@$(SCRIPTS)/build.sh

run: olddefconfig
	@$(SCRIPTS)/runner.sh

clean:
	@rm -rf $(OUT)/*

repo-init:
	@repo init -m default.xml -u $(git rev-parse --show-toplevel) -b $(git rev-parse HEAD)
	@repo sync -j4