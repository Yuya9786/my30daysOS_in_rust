;a_nasm.asm
global api_putchar

section .text

api_putchar:
	mov edx,1
	mov al, [esp+4]
	int 0x40
	ret