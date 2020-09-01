[BITS 32]						; 32ビットモード用の機械語を作らせる

  GLOBAL _asm_console_putchar

_asm_console_putchar:
    PUSH    1
    AND     EAX,0xff    ; AHやEAXの上位を0にして，EAXに文字コードが入った状態にする．
    PUSH    EAX
    PUSH    DWORD [0x0fec]  ; メモリの内容を読み込んでその値をPUSHする
    CALL    _put_chr
    AND     ESP,12      ;スタックに積んだデータを捨てる
    RET
