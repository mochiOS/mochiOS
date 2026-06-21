SCRIPTS	= $(shell pwd)/scripts
OUT		= $(shell pwd)/out

.PHONY: all build run clean
all: build

build:
	@$(SCRIPTS)/build.sh

run:
	@$(SCRIPTS)/run.sh

clean:
	@rm -rf $(OUT)/*
