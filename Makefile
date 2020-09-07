OUTPUT_DIR := build
ASM_DIR := asm
OUTPUT_DIR_KEEP := $(OUTPUT_DIR)/.keep
IMG := $(OUTPUT_DIR)/haribote.img
CSRC := csrc

default:
	make img

$(OUTPUT_DIR)/a_nasm.bin: $(ASM_DIR)/a_nasm.asm Makefile $(OUTPUT_DIR_KEEP)
	nasm -f elf32 $< -o $@

$(OUTPUT_DIR)/%.bin: $(ASM_DIR)/%.asm Makefile $(OUTPUT_DIR_KEEP)
	nasm $< -o $@

$(OUTPUT_DIR)/haribote.sys: $(OUTPUT_DIR)/asmhead.bin $(OUTPUT_DIR)/kernel.bin
	cat $^ > $@

$(IMG) : $(OUTPUT_DIR)/ipl10.bin $(OUTPUT_DIR)/haribote.sys $(OUTPUT_DIR)/hlt.bin $(OUTPUT_DIR)/hello.bin $(OUTPUT_DIR)/hello2.bin $(OUTPUT_DIR)/hello3.hrb Makefile
	mformat -f 1440 -C -B $< -i $@ ::
	mcopy -i $@ src/test.txt ::
	mcopy -i $@ $(OUTPUT_DIR)/hlt.bin ::
	mcopy -i $@ $(OUTPUT_DIR)/hello.bin ::
	mcopy -i $@ $(OUTPUT_DIR)/hello2.bin ::

$(OUTPUT_DIR)/%.o : $(CSRC)/%.c Makefile $(OUTPUT_DIR_KEEP)
	x86_64-elf-gcc -c -m32 -o $@ $<

asm :
	make $(OUTPUT_DIR)/ipl10.bin 

img :
	make $(IMG)

run :
	make img
	qemu-system-i386 -m 32 -fda $(IMG) -no-reboot

debug :
	make img
	qemu-system-i386 -fda $(IMG) -gdb tcp::10000 -S

clean :
	rm -rf $(OUTPUT_DIR)/*

$(OUTPUT_DIR)/nasmfunc.o: $(ASM_DIR)/nasmfunc.asm Makefile $(OUTPUT_DIR_KEEP)
	nasm -f elf $< -o $@

$(OUTPUT_DIR)/kernel.bin: $(OUTPUT_DIR)/libharibote_os.a $(OUTPUT_DIR_KEEP)
	i686-unknown-linux-gnu-ld -v -nostdlib -Tdata=0x00310000 -Thrb.ld $< -o $@

$(OUTPUT_DIR)/%.hrb: $(OUTPUT_DIR)/%.o $(OUTPUT_DIR)/a_nasm.bin $(OUTPUT_DIR_KEEP)
	i686-unknown-linux-gnu-ld -m elf_i386 -e HariMain -o $@ -Tapi.ld $< $(OUTPUT_DIR)/a_nasm.bin

$(OUTPUT_DIR)/libharibote_os.a: $(OUTPUT_DIR_KEEP)
	cargo xbuild --target-dir $(OUTPUT_DIR)
	cp $(OUTPUT_DIR)/i386-haribote/debug/libharibote_os.a $(OUTPUT_DIR)/

$(OUTPUT_DIR_KEEP):
	mkdir -p $(OUTPUT_DIR)
	touch $@
