bits 16
org 0x600
start:
    cli
    xor ax,ax
    mov ds,ax
    mov es,ax
    mov ss,ax
    mov sp,0x7c00
    mov si,sp
    mov di,0x600
    mov cx,256
    cld
    rep movsw
    jmp 0:next
next:
    mov si,0x600+446
    mov cx,4
.l: test byte [si],0x80
    jnz .found
    add si,16
    loop .l
    hlt
.found:
    mov eax,[si+8]
    mov [dap_lba],eax
    mov si,dap
    mov ah,0x42
    mov dl,0x80
    int 0x13
    jc $
    jmp 0:0x7c00
dap: db 16,0
    dw 1
    dw 0x7c00,0
dap_lba: dq 0
