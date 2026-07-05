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
	@echo "menuconfig TUI is not implemented yet; run make olddefconfig for now."

build: olddefconfig
	@$(SCRIPTS)/build.sh

run: olddefconfig
	@$(SCRIPTS)/runner.sh

clean:
	@rm -rf $(OUT)/*

repo-init:
	@repo init -m default.xml -u $(git rev-parse --show-toplevel) -b $(git rev-parse HEAD)
	@repo sync -j4
