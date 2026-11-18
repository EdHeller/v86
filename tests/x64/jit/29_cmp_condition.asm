; EXPECT: pass
; DESC: 32-bit JIT: cmp of 8, 16 and 32 bits (a memory operand against a register, and the reverse) whose flags only the next conditional jump reads (the next block overwrites them): the jump's condition computed at the cmp (Instruction::Condition) for b ae e ne be a s ns l ge le g; branch counts from a model, interpreted and compiled
%include "inc/pm32.inc"

test_main:
    mov ebp, 20000
.rep:
    xor edi, edi
.pair:
    mov eax, [pairs_a + edi * 4]
    mov [mem], eax
    mov eax, [pairs_b + edi * 4]
    call body8
    call body16
    call body32
    inc edi
    cmp edi, 8
    jne .pair
    dec ebp
    jnz .rep
    CHECK16 1, dword [counts + 0], 80000
    CHECK16 2, dword [counts + 4], 120000
    CHECK16 3, dword [counts + 8], 80000
    CHECK16 4, dword [counts + 12], 40000
    CHECK16 5, dword [counts + 16], 120000
    CHECK16 6, dword [counts + 20], 120000
    CHECK16 7, dword [counts + 24], 40000
    CHECK16 8, dword [counts + 28], 40000
    CHECK16 9, dword [counts + 32], 40000
    CHECK16 10, dword [counts + 36], 80000
    CHECK16 11, dword [counts + 40], 120000
    CHECK16 12, dword [counts + 44], 80000
    CHECK16 13, dword [counts + 48], 100000
    CHECK16 14, dword [counts + 52], 100000
    CHECK16 15, dword [counts + 56], 60000
    CHECK16 16, dword [counts + 60], 60000
    CHECK16 17, dword [counts + 64], 100000
    CHECK16 18, dword [counts + 68], 100000
    CHECK16 19, dword [counts + 72], 60000
    CHECK16 20, dword [counts + 76], 60000
    CHECK16 21, dword [counts + 80], 60000
    CHECK16 22, dword [counts + 84], 60000
    CHECK16 23, dword [counts + 88], 100000
    CHECK16 24, dword [counts + 92], 100000
    CHECK16 25, dword [counts + 96], 100000
    CHECK16 26, dword [counts + 100], 100000
    CHECK16 27, dword [counts + 104], 60000
    CHECK16 28, dword [counts + 108], 60000
    CHECK16 29, dword [counts + 112], 120000
    CHECK16 30, dword [counts + 116], 120000
    CHECK16 31, dword [counts + 120], 40000
    CHECK16 32, dword [counts + 124], 40000
    CHECK16 33, dword [counts + 128], 60000
    CHECK16 34, dword [counts + 132], 60000
    CHECK16 35, dword [counts + 136], 100000
    CHECK16 36, dword [counts + 140], 100000
    CHECK16 37, dword [counts + 144], 100000
    CHECK16 38, dword [counts + 148], 100000
    CHECK16 39, dword [counts + 152], 60000
    CHECK16 40, dword [counts + 156], 60000
    CHECK16 41, dword [counts + 160], 80000
    CHECK16 42, dword [counts + 164], 120000
    CHECK16 43, dword [counts + 168], 80000
    CHECK16 44, dword [counts + 172], 40000
    CHECK16 45, dword [counts + 176], 40000
    CHECK16 46, dword [counts + 180], 80000
    CHECK16 47, dword [counts + 184], 120000
    CHECK16 48, dword [counts + 188], 80000
    CHECK16 49, dword [counts + 192], 100000
    CHECK16 50, dword [counts + 196], 100000
    CHECK16 51, dword [counts + 200], 60000
    CHECK16 52, dword [counts + 204], 60000
    CHECK16 53, dword [counts + 208], 120000
    CHECK16 54, dword [counts + 212], 120000
    CHECK16 55, dword [counts + 216], 40000
    CHECK16 56, dword [counts + 220], 40000
    CHECK16 57, dword [counts + 224], 60000
    CHECK16 58, dword [counts + 228], 60000
    CHECK16 59, dword [counts + 232], 100000
    CHECK16 60, dword [counts + 236], 100000
    CHECK16 61, dword [counts + 240], 80000
    CHECK16 62, dword [counts + 244], 120000
    CHECK16 63, dword [counts + 248], 80000
    CHECK16 64, dword [counts + 252], 40000
    CHECK16 65, dword [counts + 256], 80000
    CHECK16 66, dword [counts + 260], 120000
    CHECK16 67, dword [counts + 264], 80000
    CHECK16 68, dword [counts + 268], 40000
    CHECK16 69, dword [counts + 272], 40000
    CHECK16 70, dword [counts + 276], 80000
    CHECK16 71, dword [counts + 280], 120000
    CHECK16 72, dword [counts + 284], 80000
    PASS

align 4096
body8:
    cmp byte [mem], al
    jb t0
    add dword [counts + 0], 1
t0:
    cmp al, byte [mem]
    jb t1
    add dword [counts + 4], 1
t1:
    cmp byte [mem], al
    jae t2
    add dword [counts + 8], 1
t2:
    cmp al, byte [mem]
    jae t3
    add dword [counts + 12], 1
t3:
    cmp byte [mem], al
    je t4
    add dword [counts + 16], 1
t4:
    cmp al, byte [mem]
    je t5
    add dword [counts + 20], 1
t5:
    cmp byte [mem], al
    jne t6
    add dword [counts + 24], 1
t6:
    cmp al, byte [mem]
    jne t7
    add dword [counts + 28], 1
t7:
    cmp byte [mem], al
    jbe t8
    add dword [counts + 32], 1
t8:
    cmp al, byte [mem]
    jbe t9
    add dword [counts + 36], 1
t9:
    cmp byte [mem], al
    ja t10
    add dword [counts + 40], 1
t10:
    cmp al, byte [mem]
    ja t11
    add dword [counts + 44], 1
t11:
    cmp byte [mem], al
    js t12
    add dword [counts + 48], 1
t12:
    cmp al, byte [mem]
    js t13
    add dword [counts + 52], 1
t13:
    cmp byte [mem], al
    jns t14
    add dword [counts + 56], 1
t14:
    cmp al, byte [mem]
    jns t15
    add dword [counts + 60], 1
t15:
    cmp byte [mem], al
    jl t16
    add dword [counts + 64], 1
t16:
    cmp al, byte [mem]
    jl t17
    add dword [counts + 68], 1
t17:
    cmp byte [mem], al
    jge t18
    add dword [counts + 72], 1
t18:
    cmp al, byte [mem]
    jge t19
    add dword [counts + 76], 1
t19:
    cmp byte [mem], al
    jle t20
    add dword [counts + 80], 1
t20:
    cmp al, byte [mem]
    jle t21
    add dword [counts + 84], 1
t21:
    cmp byte [mem], al
    jg t22
    add dword [counts + 88], 1
t22:
    cmp al, byte [mem]
    jg t23
    add dword [counts + 92], 1
t23:
    ret

align 4096
body16:
    cmp word [mem], ax
    jb t24
    add dword [counts + 96], 1
t24:
    cmp ax, word [mem]
    jb t25
    add dword [counts + 100], 1
t25:
    cmp word [mem], ax
    jae t26
    add dword [counts + 104], 1
t26:
    cmp ax, word [mem]
    jae t27
    add dword [counts + 108], 1
t27:
    cmp word [mem], ax
    je t28
    add dword [counts + 112], 1
t28:
    cmp ax, word [mem]
    je t29
    add dword [counts + 116], 1
t29:
    cmp word [mem], ax
    jne t30
    add dword [counts + 120], 1
t30:
    cmp ax, word [mem]
    jne t31
    add dword [counts + 124], 1
t31:
    cmp word [mem], ax
    jbe t32
    add dword [counts + 128], 1
t32:
    cmp ax, word [mem]
    jbe t33
    add dword [counts + 132], 1
t33:
    cmp word [mem], ax
    ja t34
    add dword [counts + 136], 1
t34:
    cmp ax, word [mem]
    ja t35
    add dword [counts + 140], 1
t35:
    cmp word [mem], ax
    js t36
    add dword [counts + 144], 1
t36:
    cmp ax, word [mem]
    js t37
    add dword [counts + 148], 1
t37:
    cmp word [mem], ax
    jns t38
    add dword [counts + 152], 1
t38:
    cmp ax, word [mem]
    jns t39
    add dword [counts + 156], 1
t39:
    cmp word [mem], ax
    jl t40
    add dword [counts + 160], 1
t40:
    cmp ax, word [mem]
    jl t41
    add dword [counts + 164], 1
t41:
    cmp word [mem], ax
    jge t42
    add dword [counts + 168], 1
t42:
    cmp ax, word [mem]
    jge t43
    add dword [counts + 172], 1
t43:
    cmp word [mem], ax
    jle t44
    add dword [counts + 176], 1
t44:
    cmp ax, word [mem]
    jle t45
    add dword [counts + 180], 1
t45:
    cmp word [mem], ax
    jg t46
    add dword [counts + 184], 1
t46:
    cmp ax, word [mem]
    jg t47
    add dword [counts + 188], 1
t47:
    ret

align 4096
body32:
    cmp dword [mem], eax
    jb t48
    add dword [counts + 192], 1
t48:
    cmp eax, dword [mem]
    jb t49
    add dword [counts + 196], 1
t49:
    cmp dword [mem], eax
    jae t50
    add dword [counts + 200], 1
t50:
    cmp eax, dword [mem]
    jae t51
    add dword [counts + 204], 1
t51:
    cmp dword [mem], eax
    je t52
    add dword [counts + 208], 1
t52:
    cmp eax, dword [mem]
    je t53
    add dword [counts + 212], 1
t53:
    cmp dword [mem], eax
    jne t54
    add dword [counts + 216], 1
t54:
    cmp eax, dword [mem]
    jne t55
    add dword [counts + 220], 1
t55:
    cmp dword [mem], eax
    jbe t56
    add dword [counts + 224], 1
t56:
    cmp eax, dword [mem]
    jbe t57
    add dword [counts + 228], 1
t57:
    cmp dword [mem], eax
    ja t58
    add dword [counts + 232], 1
t58:
    cmp eax, dword [mem]
    ja t59
    add dword [counts + 236], 1
t59:
    cmp dword [mem], eax
    js t60
    add dword [counts + 240], 1
t60:
    cmp eax, dword [mem]
    js t61
    add dword [counts + 244], 1
t61:
    cmp dword [mem], eax
    jns t62
    add dword [counts + 248], 1
t62:
    cmp eax, dword [mem]
    jns t63
    add dword [counts + 252], 1
t63:
    cmp dword [mem], eax
    jl t64
    add dword [counts + 256], 1
t64:
    cmp eax, dword [mem]
    jl t65
    add dword [counts + 260], 1
t65:
    cmp dword [mem], eax
    jge t66
    add dword [counts + 264], 1
t66:
    cmp eax, dword [mem]
    jge t67
    add dword [counts + 268], 1
t67:
    cmp dword [mem], eax
    jle t68
    add dword [counts + 272], 1
t68:
    cmp eax, dword [mem]
    jle t69
    add dword [counts + 276], 1
t69:
    cmp dword [mem], eax
    jg t70
    add dword [counts + 280], 1
t70:
    cmp eax, dword [mem]
    jg t71
    add dword [counts + 284], 1
t71:
    ret

align 4096
pairs_a: dd 0x0, 0x7f, 0x80, 0xff, 0x1, 0x40, 0x12345680, 0xffff8000
pairs_b: dd 0x0, 0x80, 0x7f, 0x1, 0xff, 0x40, 0x123456ff, 0x7fff
mem: dd 0
counts: times 72 dd 0
