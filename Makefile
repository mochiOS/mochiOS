# Product assembly only. Software builds are owned by mdp workspace producers.
MMAKE ?= $(error mochiOS requires mmake)
MMAKE_REQUIRED := $(MMAKE)
OUT := $(ROOT)/out
MMAKE_OUT := $(OUT)/mmake
SCRIPTS := $(ROOT)/scripts
HOST_TOOLS_DIR := $(MMAKE_OUT)/host-tools
MSIGN := $(HOST_TOOLS_DIR)/release/msign
MPACK := $(HOST_TOOLS_DIR)/release/mpack

include build/mmake/config.mk
include build/mmake/development-tools.mk
include build/mmake/image.mk

all: image
build: image

# Linux UI preview is an explicit host-development action, not an OS image input.
include tools/devkit/build-support/toolchain.mk
include build/mmake/preview.mk
include build/mmake/tests.mk
ime-dictionary:
	@always
	test -f $(MMAKE_OUT)/components/ime/ja.mime || { echo 'Run mdp workspace build --select image-data.' >&2; exit 1; }

include build/help.make
