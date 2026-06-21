SCRIPTS	= $(shell pwd)/scripts
OUT		= $(shell pwd)/out

.PHONY: all clean
all: run

run:
	@$(SCRIPTS)/runner.sh

clean:
	@rm -rf $(OUT)/*