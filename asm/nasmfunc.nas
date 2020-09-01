extern _console_putchr

_asm_console_putchr:
    PUSH    1
    AND     EAX,0xff    ; AHやEAXの上位を0にして，EAXに文字コードが入った状態にする．
    PUSH    EAX
    PUSH    DWORD [0x0fec]  ; メモリの内容を読み込んでその値をPUSHする
    CALL    _console_putchar
    AND     ESP,12      ;スタックに積んだデータを捨てる
    RET
