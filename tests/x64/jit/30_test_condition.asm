; EXPECT: pass
; DESC: 32-bit JIT: test of 8, 16 and 32 bits (memory and register, register with itself, register and immediate) whose flags only the next conditional jump reads: the jump's condition computed at the test (Instruction::Condition); branch counts from a model, interpreted and compiled
%include "inc/pm32.inc"

test_main:
    mov ebp, 20000
.rep:
    xor edi, edi
.pair:
    mov eax, [pairs_a + edi * 4]
    mov [mem], eax
    mov ebx, eax
    mov eax, [pairs_b + edi * 4]
    call body8
    call body16
    call body32
    inc edi
    cmp edi, 8
    jne .pair
    dec ebp
    jnz .rep
    CHECK16 1, dword [counts + 0], 160000
    CHECK16 2, dword [counts + 4], 160000
    CHECK16 3, dword [counts + 8], 160000
    CHECK16 4, dword [counts + 12], 0
    CHECK16 5, dword [counts + 16], 0
    CHECK16 6, dword [counts + 20], 0
    CHECK16 7, dword [counts + 24], 160000
    CHECK16 8, dword [counts + 28], 160000
    CHECK16 9, dword [counts + 32], 160000
    CHECK16 10, dword [counts + 36], 0
    CHECK16 11, dword [counts + 40], 0
    CHECK16 12, dword [counts + 44], 0
    CHECK16 13, dword [counts + 48], 80000
    CHECK16 14, dword [counts + 52], 100000
    CHECK16 15, dword [counts + 56], 100000
    CHECK16 16, dword [counts + 60], 80000
    CHECK16 17, dword [counts + 64], 60000
    CHECK16 18, dword [counts + 68], 60000
    CHECK16 19, dword [counts + 72], 80000
    CHECK16 20, dword [counts + 76], 100000
    CHECK16 21, dword [counts + 80], 100000
    CHECK16 22, dword [counts + 84], 80000
    CHECK16 23, dword [counts + 88], 60000
    CHECK16 24, dword [counts + 92], 60000
    CHECK16 25, dword [counts + 96], 140000
    CHECK16 26, dword [counts + 100], 120000
    CHECK16 27, dword [counts + 104], 60000
    CHECK16 28, dword [counts + 108], 20000
    CHECK16 29, dword [counts + 112], 40000
    CHECK16 30, dword [counts + 116], 100000
    CHECK16 31, dword [counts + 120], 140000
    CHECK16 32, dword [counts + 124], 120000
    CHECK16 33, dword [counts + 128], 60000
    CHECK16 34, dword [counts + 132], 20000
    CHECK16 35, dword [counts + 136], 40000
    CHECK16 36, dword [counts + 140], 100000
    CHECK16 37, dword [counts + 144], 60000
    CHECK16 38, dword [counts + 148], 60000
    CHECK16 39, dword [counts + 152], 0
    CHECK16 40, dword [counts + 156], 100000
    CHECK16 41, dword [counts + 160], 100000
    CHECK16 42, dword [counts + 164], 160000
    CHECK16 43, dword [counts + 168], 160000
    CHECK16 44, dword [counts + 172], 160000
    CHECK16 45, dword [counts + 176], 160000
    CHECK16 46, dword [counts + 180], 0
    CHECK16 47, dword [counts + 184], 0
    CHECK16 48, dword [counts + 188], 0
    CHECK16 49, dword [counts + 192], 160000
    CHECK16 50, dword [counts + 196], 160000
    CHECK16 51, dword [counts + 200], 160000
    CHECK16 52, dword [counts + 204], 0
    CHECK16 53, dword [counts + 208], 0
    CHECK16 54, dword [counts + 212], 0
    CHECK16 55, dword [counts + 216], 100000
    CHECK16 56, dword [counts + 220], 120000
    CHECK16 57, dword [counts + 224], 100000
    CHECK16 58, dword [counts + 228], 60000
    CHECK16 59, dword [counts + 232], 40000
    CHECK16 60, dword [counts + 236], 60000
    CHECK16 61, dword [counts + 240], 100000
    CHECK16 62, dword [counts + 244], 120000
    CHECK16 63, dword [counts + 248], 100000
    CHECK16 64, dword [counts + 252], 60000
    CHECK16 65, dword [counts + 256], 40000
    CHECK16 66, dword [counts + 260], 60000
    CHECK16 67, dword [counts + 264], 140000
    CHECK16 68, dword [counts + 268], 140000
    CHECK16 69, dword [counts + 272], 160000
    CHECK16 70, dword [counts + 276], 20000
    CHECK16 71, dword [counts + 280], 20000
    CHECK16 72, dword [counts + 284], 0
    CHECK16 73, dword [counts + 288], 140000
    CHECK16 74, dword [counts + 292], 140000
    CHECK16 75, dword [counts + 296], 160000
    CHECK16 76, dword [counts + 300], 20000
    CHECK16 77, dword [counts + 304], 20000
    CHECK16 78, dword [counts + 308], 0
    CHECK16 79, dword [counts + 312], 80000
    CHECK16 80, dword [counts + 316], 100000
    CHECK16 81, dword [counts + 320], 100000
    CHECK16 82, dword [counts + 324], 80000
    CHECK16 83, dword [counts + 328], 60000
    CHECK16 84, dword [counts + 332], 60000
    CHECK16 85, dword [counts + 336], 160000
    CHECK16 86, dword [counts + 340], 160000
    CHECK16 87, dword [counts + 344], 160000
    CHECK16 88, dword [counts + 348], 0
    CHECK16 89, dword [counts + 352], 0
    CHECK16 90, dword [counts + 356], 0
    CHECK16 91, dword [counts + 360], 160000
    CHECK16 92, dword [counts + 364], 160000
    CHECK16 93, dword [counts + 368], 160000
    CHECK16 94, dword [counts + 372], 0
    CHECK16 95, dword [counts + 376], 0
    CHECK16 96, dword [counts + 380], 0
    CHECK16 97, dword [counts + 384], 120000
    CHECK16 98, dword [counts + 388], 140000
    CHECK16 99, dword [counts + 392], 100000
    CHECK16 100, dword [counts + 396], 40000
    CHECK16 101, dword [counts + 400], 20000
    CHECK16 102, dword [counts + 404], 60000
    CHECK16 103, dword [counts + 408], 120000
    CHECK16 104, dword [counts + 412], 140000
    CHECK16 105, dword [counts + 416], 100000
    CHECK16 106, dword [counts + 420], 40000
    CHECK16 107, dword [counts + 424], 20000
    CHECK16 108, dword [counts + 428], 60000
    CHECK16 109, dword [counts + 432], 140000
    CHECK16 110, dword [counts + 436], 140000
    CHECK16 111, dword [counts + 440], 160000
    CHECK16 112, dword [counts + 444], 20000
    CHECK16 113, dword [counts + 448], 20000
    CHECK16 114, dword [counts + 452], 0
    CHECK16 115, dword [counts + 456], 140000
    CHECK16 116, dword [counts + 460], 140000
    CHECK16 117, dword [counts + 464], 160000
    CHECK16 118, dword [counts + 468], 20000
    CHECK16 119, dword [counts + 472], 20000
    CHECK16 120, dword [counts + 476], 0
    CHECK16 121, dword [counts + 480], 100000
    CHECK16 122, dword [counts + 484], 120000
    CHECK16 123, dword [counts + 488], 100000
    CHECK16 124, dword [counts + 492], 60000
    CHECK16 125, dword [counts + 496], 40000
    CHECK16 126, dword [counts + 500], 60000
    PASS

align 4096
body8:
    test byte [mem], al
    jo t0
    add dword [counts + 0], 1
t0:
    test bl, bl
    jo t1
    add dword [counts + 4], 1
t1:
    test al, 0x80
    jo t2
    add dword [counts + 8], 1
t2:
    test byte [mem], al
    jno t3
    add dword [counts + 12], 1
t3:
    test bl, bl
    jno t4
    add dword [counts + 16], 1
t4:
    test al, 0x80
    jno t5
    add dword [counts + 20], 1
t5:
    test byte [mem], al
    jb t6
    add dword [counts + 24], 1
t6:
    test bl, bl
    jb t7
    add dword [counts + 28], 1
t7:
    test al, 0x80
    jb t8
    add dword [counts + 32], 1
t8:
    test byte [mem], al
    jae t9
    add dword [counts + 36], 1
t9:
    test bl, bl
    jae t10
    add dword [counts + 40], 1
t10:
    test al, 0x80
    jae t11
    add dword [counts + 44], 1
t11:
    test byte [mem], al
    je t12
    add dword [counts + 48], 1
t12:
    test bl, bl
    je t13
    add dword [counts + 52], 1
t13:
    test al, 0x80
    je t14
    add dword [counts + 56], 1
t14:
    test byte [mem], al
    jne t15
    add dword [counts + 60], 1
t15:
    test bl, bl
    jne t16
    add dword [counts + 64], 1
t16:
    test al, 0x80
    jne t17
    add dword [counts + 68], 1
t17:
    test byte [mem], al
    jbe t18
    add dword [counts + 72], 1
t18:
    test bl, bl
    jbe t19
    add dword [counts + 76], 1
t19:
    test al, 0x80
    jbe t20
    add dword [counts + 80], 1
t20:
    test byte [mem], al
    ja t21
    add dword [counts + 84], 1
t21:
    test bl, bl
    ja t22
    add dword [counts + 88], 1
t22:
    test al, 0x80
    ja t23
    add dword [counts + 92], 1
t23:
    test byte [mem], al
    js t24
    add dword [counts + 96], 1
t24:
    test bl, bl
    js t25
    add dword [counts + 100], 1
t25:
    test al, 0x80
    js t26
    add dword [counts + 104], 1
t26:
    test byte [mem], al
    jns t27
    add dword [counts + 108], 1
t27:
    test bl, bl
    jns t28
    add dword [counts + 112], 1
t28:
    test al, 0x80
    jns t29
    add dword [counts + 116], 1
t29:
    test byte [mem], al
    jl t30
    add dword [counts + 120], 1
t30:
    test bl, bl
    jl t31
    add dword [counts + 124], 1
t31:
    test al, 0x80
    jl t32
    add dword [counts + 128], 1
t32:
    test byte [mem], al
    jge t33
    add dword [counts + 132], 1
t33:
    test bl, bl
    jge t34
    add dword [counts + 136], 1
t34:
    test al, 0x80
    jge t35
    add dword [counts + 140], 1
t35:
    test byte [mem], al
    jle t36
    add dword [counts + 144], 1
t36:
    test bl, bl
    jle t37
    add dword [counts + 148], 1
t37:
    test al, 0x80
    jle t38
    add dword [counts + 152], 1
t38:
    test byte [mem], al
    jg t39
    add dword [counts + 156], 1
t39:
    test bl, bl
    jg t40
    add dword [counts + 160], 1
t40:
    test al, 0x80
    jg t41
    add dword [counts + 164], 1
t41:
    ret

align 4096
body16:
    test word [mem], ax
    jo t42
    add dword [counts + 168], 1
t42:
    test bx, bx
    jo t43
    add dword [counts + 172], 1
t43:
    test ax, 0x80
    jo t44
    add dword [counts + 176], 1
t44:
    test word [mem], ax
    jno t45
    add dword [counts + 180], 1
t45:
    test bx, bx
    jno t46
    add dword [counts + 184], 1
t46:
    test ax, 0x80
    jno t47
    add dword [counts + 188], 1
t47:
    test word [mem], ax
    jb t48
    add dword [counts + 192], 1
t48:
    test bx, bx
    jb t49
    add dword [counts + 196], 1
t49:
    test ax, 0x80
    jb t50
    add dword [counts + 200], 1
t50:
    test word [mem], ax
    jae t51
    add dword [counts + 204], 1
t51:
    test bx, bx
    jae t52
    add dword [counts + 208], 1
t52:
    test ax, 0x80
    jae t53
    add dword [counts + 212], 1
t53:
    test word [mem], ax
    je t54
    add dword [counts + 216], 1
t54:
    test bx, bx
    je t55
    add dword [counts + 220], 1
t55:
    test ax, 0x80
    je t56
    add dword [counts + 224], 1
t56:
    test word [mem], ax
    jne t57
    add dword [counts + 228], 1
t57:
    test bx, bx
    jne t58
    add dword [counts + 232], 1
t58:
    test ax, 0x80
    jne t59
    add dword [counts + 236], 1
t59:
    test word [mem], ax
    jbe t60
    add dword [counts + 240], 1
t60:
    test bx, bx
    jbe t61
    add dword [counts + 244], 1
t61:
    test ax, 0x80
    jbe t62
    add dword [counts + 248], 1
t62:
    test word [mem], ax
    ja t63
    add dword [counts + 252], 1
t63:
    test bx, bx
    ja t64
    add dword [counts + 256], 1
t64:
    test ax, 0x80
    ja t65
    add dword [counts + 260], 1
t65:
    test word [mem], ax
    js t66
    add dword [counts + 264], 1
t66:
    test bx, bx
    js t67
    add dword [counts + 268], 1
t67:
    test ax, 0x80
    js t68
    add dword [counts + 272], 1
t68:
    test word [mem], ax
    jns t69
    add dword [counts + 276], 1
t69:
    test bx, bx
    jns t70
    add dword [counts + 280], 1
t70:
    test ax, 0x80
    jns t71
    add dword [counts + 284], 1
t71:
    test word [mem], ax
    jl t72
    add dword [counts + 288], 1
t72:
    test bx, bx
    jl t73
    add dword [counts + 292], 1
t73:
    test ax, 0x80
    jl t74
    add dword [counts + 296], 1
t74:
    test word [mem], ax
    jge t75
    add dword [counts + 300], 1
t75:
    test bx, bx
    jge t76
    add dword [counts + 304], 1
t76:
    test ax, 0x80
    jge t77
    add dword [counts + 308], 1
t77:
    test word [mem], ax
    jle t78
    add dword [counts + 312], 1
t78:
    test bx, bx
    jle t79
    add dword [counts + 316], 1
t79:
    test ax, 0x80
    jle t80
    add dword [counts + 320], 1
t80:
    test word [mem], ax
    jg t81
    add dword [counts + 324], 1
t81:
    test bx, bx
    jg t82
    add dword [counts + 328], 1
t82:
    test ax, 0x80
    jg t83
    add dword [counts + 332], 1
t83:
    ret

align 4096
body32:
    test dword [mem], eax
    jo t84
    add dword [counts + 336], 1
t84:
    test ebx, ebx
    jo t85
    add dword [counts + 340], 1
t85:
    test eax, 0x80
    jo t86
    add dword [counts + 344], 1
t86:
    test dword [mem], eax
    jno t87
    add dword [counts + 348], 1
t87:
    test ebx, ebx
    jno t88
    add dword [counts + 352], 1
t88:
    test eax, 0x80
    jno t89
    add dword [counts + 356], 1
t89:
    test dword [mem], eax
    jb t90
    add dword [counts + 360], 1
t90:
    test ebx, ebx
    jb t91
    add dword [counts + 364], 1
t91:
    test eax, 0x80
    jb t92
    add dword [counts + 368], 1
t92:
    test dword [mem], eax
    jae t93
    add dword [counts + 372], 1
t93:
    test ebx, ebx
    jae t94
    add dword [counts + 376], 1
t94:
    test eax, 0x80
    jae t95
    add dword [counts + 380], 1
t95:
    test dword [mem], eax
    je t96
    add dword [counts + 384], 1
t96:
    test ebx, ebx
    je t97
    add dword [counts + 388], 1
t97:
    test eax, 0x80
    je t98
    add dword [counts + 392], 1
t98:
    test dword [mem], eax
    jne t99
    add dword [counts + 396], 1
t99:
    test ebx, ebx
    jne t100
    add dword [counts + 400], 1
t100:
    test eax, 0x80
    jne t101
    add dword [counts + 404], 1
t101:
    test dword [mem], eax
    jbe t102
    add dword [counts + 408], 1
t102:
    test ebx, ebx
    jbe t103
    add dword [counts + 412], 1
t103:
    test eax, 0x80
    jbe t104
    add dword [counts + 416], 1
t104:
    test dword [mem], eax
    ja t105
    add dword [counts + 420], 1
t105:
    test ebx, ebx
    ja t106
    add dword [counts + 424], 1
t106:
    test eax, 0x80
    ja t107
    add dword [counts + 428], 1
t107:
    test dword [mem], eax
    js t108
    add dword [counts + 432], 1
t108:
    test ebx, ebx
    js t109
    add dword [counts + 436], 1
t109:
    test eax, 0x80
    js t110
    add dword [counts + 440], 1
t110:
    test dword [mem], eax
    jns t111
    add dword [counts + 444], 1
t111:
    test ebx, ebx
    jns t112
    add dword [counts + 448], 1
t112:
    test eax, 0x80
    jns t113
    add dword [counts + 452], 1
t113:
    test dword [mem], eax
    jl t114
    add dword [counts + 456], 1
t114:
    test ebx, ebx
    jl t115
    add dword [counts + 460], 1
t115:
    test eax, 0x80
    jl t116
    add dword [counts + 464], 1
t116:
    test dword [mem], eax
    jge t117
    add dword [counts + 468], 1
t117:
    test ebx, ebx
    jge t118
    add dword [counts + 472], 1
t118:
    test eax, 0x80
    jge t119
    add dword [counts + 476], 1
t119:
    test dword [mem], eax
    jle t120
    add dword [counts + 480], 1
t120:
    test ebx, ebx
    jle t121
    add dword [counts + 484], 1
t121:
    test eax, 0x80
    jle t122
    add dword [counts + 488], 1
t122:
    test dword [mem], eax
    jg t123
    add dword [counts + 492], 1
t123:
    test ebx, ebx
    jg t124
    add dword [counts + 496], 1
t124:
    test eax, 0x80
    jg t125
    add dword [counts + 500], 1
t125:
    ret

align 4096
pairs_a: dd 0x0, 0x7f, 0x80, 0xff, 0x1, 0x40, 0x12348000, 0x80000000
pairs_b: dd 0x0, 0x80, 0xff, 0x1, 0xff, 0x40, 0x1234ffff, 0xffffffff
mem: dd 0
counts: times 126 dd 0
