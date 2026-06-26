SCRIPTS	= $(shell pwd)/scripts
OUT		= $(shell pwd)/out

.PHONY: all build run clean repo-init
all: build

build:
	@$(SCRIPTS)/build.sh

run:
	@$(SCRIPTS)/runner.sh

clean:
	@rm -rf $(OUT)/*

repo-init:
	@repo init -m default.xml -u $(git rev-parse --show-toplevel) -b $(git rev-parse HEAD)
	@repo sync -j4