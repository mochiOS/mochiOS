SCRIPTS = $(shell pwd)/scripts
OUT     = $(shell pwd)/out

MANIFEST_URL    ?= https://github.com/mochiOS/mochiOS.git
MANIFEST_BRANCH ?= master
MANIFEST_FILE   ?= default.xml
REPO_JOBS       ?= 4

.PHONY: all build run clean olddefconfig menuconfig repo-init repo

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
	@repo init \
		-u $(MANIFEST_URL) \
		-b $(MANIFEST_BRANCH) \
		-m $(MANIFEST_FILE)
	@repo sync -j$(REPO_JOBS)

repo:
	@repo sync -j$(REPO_JOBS)