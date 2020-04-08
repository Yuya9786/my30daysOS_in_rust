OUTPUT_DIR := build
ASM_DIR := asm
OUTPUT_DIR_KEEP := $(OUTPUT_DIR)/.keep
IMG := $(OUTPUT_DIR)/haribote.img

default:
	make img

$(OUTPUT_DIR)/ipl10.bin: $(ASM_DIR)/ipl10.asm Makefile 
	nasm $(ASM_DIR)/ipl10.asm -o $(OUTPUT_DIR)/ipl10.bin -l $(OUTPUT_DIR)/ipl10.list

$(OUTPUT_DIR)/asmhead.bin: $(ASM_DIR)/asmhead.asm Makefile
	nasm $(ASM_DIR)/asmhead.asm -o $(OUTPUT_DIR)/asmhead.bin -l $(OUTPUT_DIR)/asmhead.lst

$(OUTPUT_DIR)/libharibote_os.a: $(OUTPUT_DIR_KEEP)
	cargo xbuild --target-dir $(OUTPUT_DIR)
	cp $(OUTPUT_DIR)/i686-haribote/debug/libharibote_os.a $(OUTPUT_DIR)/

$(OUTPUT_DIR)/bootpack.hrb: $(OUTPUT_DIR)/libharibote_os.a hrb.ld
	i686-unknown-linux-gnu-ld -v -nostdlib -Tdata=0x00310000 -T hrb.ld $(OUTPUT_DIR)/libharibote_os.a -o $(OUTPUT_DIR)/bootpack.hrb

$(OUTPUT_DIR)/haribote.sys : $(OUTPUT_DIR)/asmhead.bin $(OUTPUT_DIR)/bootpack.hrb
	cat $(OUTPUT_DIR)/asmhead.bin $(OUTPUT_DIR)/bootpack.hrb > $(OUTPUT_DIR)/haribote.sys

$(IMG) : $(OUTPUT_DIR)/ipl10.bin $(OUTPUT_DIR)/haribote.sys Makefile
	mformat -f 1440 -C -B $(OUTPUT_DIR)/ipl10.bin -i $(IMG) ::
	mcopy -i $(IMG) $(OUTPUT_DIR)/haribote.sys ::

img :
	make $(IMG)

run :
	make img
	qemu-system-i386 -drive file=$(IMG),format=raw,if=floppy -boot a

clean :
	rm -rf $(OUTPUT_DIR)/*
