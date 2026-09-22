# mochiOS build graph for mmake.
# GNU Make is intentionally not supported by this file.

MMAKE ?= $(error mochiOS requires mmake; run 'mmake', not 'make')
MMAKE_REQUIRED := $(MMAKE)

OUT := $(ROOT)/out
MMAKE_OUT := $(OUT)/mmake
SCRIPTS := $(ROOT)/scripts

include build/mmake/config.mk
include build/mmake/inputs.mk
include build/mmake/toolchain.mk
include build/mmake/components.mk
include build/mmake/services.mk
include build/mmake/apps.mk
include build/mmake/binaries.mk
include build/mmake/extensions.mk
include build/mmake/signing.mk
include build/mmake/image.mk
include build/mmake/tests.mk
include build/mmake/preview.mk

all: image

build: image
