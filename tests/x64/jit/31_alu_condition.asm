; EXPECT: pass
; DESC: 32-bit JIT: sub, add, and, or, xor (memory destination, register destination, immediate) whose flags only the next conditional jump reads: the jump's condition computed at the instruction (Instruction::Condition) where it can be (sub: as cmp; and/or/xor: as test; add: e ne s ns), stored flags for the rest; every condition; branch counts from a model, interpreted and compiled
%include "inc/pm32.inc"

test_main:
    mov ebp, 12000
.rep:
    xor edi, edi
.pair:
    mov ebx, [pairs_a + edi * 4]
    mov eax, [pairs_b + edi * 4]
    call body_sub
    call body_add
    call body_and
    call body_or
    call body_xor
    inc edi
    cmp edi, 8
    jne .pair
    dec ebp
    jnz .rep
    CHECK16 1, dword [counts + 0], 60000
    CHECK16 2, dword [counts + 4], 60000
    CHECK16 3, dword [counts + 8], 48000
    CHECK16 4, dword [counts + 12], 36000
    CHECK16 5, dword [counts + 16], 36000
    CHECK16 6, dword [counts + 20], 48000
    CHECK16 7, dword [counts + 24], 72000
    CHECK16 8, dword [counts + 28], 72000
    CHECK16 9, dword [counts + 32], 12000
    CHECK16 10, dword [counts + 36], 24000
    CHECK16 11, dword [counts + 40], 24000
    CHECK16 12, dword [counts + 44], 84000
    CHECK16 13, dword [counts + 48], 60000
    CHECK16 14, dword [counts + 52], 60000
    CHECK16 15, dword [counts + 56], 96000
    CHECK16 16, dword [counts + 60], 36000
    CHECK16 17, dword [counts + 64], 36000
    CHECK16 18, dword [counts + 68], 0
    CHECK16 19, dword [counts + 72], 36000
    CHECK16 20, dword [counts + 76], 36000
    CHECK16 21, dword [counts + 80], 12000
    CHECK16 22, dword [counts + 84], 60000
    CHECK16 23, dword [counts + 88], 60000
    CHECK16 24, dword [counts + 92], 84000
    CHECK16 25, dword [counts + 96], 60000
    CHECK16 26, dword [counts + 100], 60000
    CHECK16 27, dword [counts + 104], 24000
    CHECK16 28, dword [counts + 108], 36000
    CHECK16 29, dword [counts + 112], 36000
    CHECK16 30, dword [counts + 116], 72000
    CHECK16 31, dword [counts + 120], 72000
    CHECK16 32, dword [counts + 124], 72000
    CHECK16 33, dword [counts + 128], 72000
    CHECK16 34, dword [counts + 132], 24000
    CHECK16 35, dword [counts + 136], 24000
    CHECK16 36, dword [counts + 140], 24000
    CHECK16 37, dword [counts + 144], 36000
    CHECK16 38, dword [counts + 148], 36000
    CHECK16 39, dword [counts + 152], 72000
    CHECK16 40, dword [counts + 156], 60000
    CHECK16 41, dword [counts + 160], 60000
    CHECK16 42, dword [counts + 164], 24000
    CHECK16 43, dword [counts + 168], 72000
    CHECK16 44, dword [counts + 172], 72000
    CHECK16 45, dword [counts + 176], 72000
    CHECK16 46, dword [counts + 180], 24000
    CHECK16 47, dword [counts + 184], 24000
    CHECK16 48, dword [counts + 188], 24000
    CHECK16 49, dword [counts + 192], 72000
    CHECK16 50, dword [counts + 196], 72000
    CHECK16 51, dword [counts + 200], 48000
    CHECK16 52, dword [counts + 204], 24000
    CHECK16 53, dword [counts + 208], 24000
    CHECK16 54, dword [counts + 212], 48000
    CHECK16 55, dword [counts + 216], 60000
    CHECK16 56, dword [counts + 220], 60000
    CHECK16 57, dword [counts + 224], 84000
    CHECK16 58, dword [counts + 228], 36000
    CHECK16 59, dword [counts + 232], 36000
    CHECK16 60, dword [counts + 236], 12000
    CHECK16 61, dword [counts + 240], 60000
    CHECK16 62, dword [counts + 244], 60000
    CHECK16 63, dword [counts + 248], 48000
    CHECK16 64, dword [counts + 252], 36000
    CHECK16 65, dword [counts + 256], 36000
    CHECK16 66, dword [counts + 260], 48000
    CHECK16 67, dword [counts + 264], 48000
    CHECK16 68, dword [counts + 268], 48000
    CHECK16 69, dword [counts + 272], 36000
    CHECK16 70, dword [counts + 276], 48000
    CHECK16 71, dword [counts + 280], 48000
    CHECK16 72, dword [counts + 284], 60000
    CHECK16 73, dword [counts + 288], 48000
    CHECK16 74, dword [counts + 292], 48000
    CHECK16 75, dword [counts + 296], 12000
    CHECK16 76, dword [counts + 300], 48000
    CHECK16 77, dword [counts + 304], 48000
    CHECK16 78, dword [counts + 308], 84000
    CHECK16 79, dword [counts + 312], 24000
    CHECK16 80, dword [counts + 316], 24000
    CHECK16 81, dword [counts + 320], 0
    CHECK16 82, dword [counts + 324], 72000
    CHECK16 83, dword [counts + 328], 72000
    CHECK16 84, dword [counts + 332], 96000
    CHECK16 85, dword [counts + 336], 96000
    CHECK16 86, dword [counts + 340], 96000
    CHECK16 87, dword [counts + 344], 96000
    CHECK16 88, dword [counts + 348], 0
    CHECK16 89, dword [counts + 352], 0
    CHECK16 90, dword [counts + 356], 0
    CHECK16 91, dword [counts + 360], 96000
    CHECK16 92, dword [counts + 364], 96000
    CHECK16 93, dword [counts + 368], 96000
    CHECK16 94, dword [counts + 372], 0
    CHECK16 95, dword [counts + 376], 0
    CHECK16 96, dword [counts + 380], 0
    CHECK16 97, dword [counts + 384], 60000
    CHECK16 98, dword [counts + 388], 60000
    CHECK16 99, dword [counts + 392], 72000
    CHECK16 100, dword [counts + 396], 36000
    CHECK16 101, dword [counts + 400], 36000
    CHECK16 102, dword [counts + 404], 24000
    CHECK16 103, dword [counts + 408], 60000
    CHECK16 104, dword [counts + 412], 60000
    CHECK16 105, dword [counts + 416], 72000
    CHECK16 106, dword [counts + 420], 36000
    CHECK16 107, dword [counts + 424], 36000
    CHECK16 108, dword [counts + 428], 24000
    CHECK16 109, dword [counts + 432], 84000
    CHECK16 110, dword [counts + 436], 84000
    CHECK16 111, dword [counts + 440], 60000
    CHECK16 112, dword [counts + 444], 12000
    CHECK16 113, dword [counts + 448], 12000
    CHECK16 114, dword [counts + 452], 36000
    CHECK16 115, dword [counts + 456], 84000
    CHECK16 116, dword [counts + 460], 84000
    CHECK16 117, dword [counts + 464], 60000
    CHECK16 118, dword [counts + 468], 12000
    CHECK16 119, dword [counts + 472], 12000
    CHECK16 120, dword [counts + 476], 36000
    CHECK16 121, dword [counts + 480], 48000
    CHECK16 122, dword [counts + 484], 48000
    CHECK16 123, dword [counts + 488], 36000
    CHECK16 124, dword [counts + 492], 48000
    CHECK16 125, dword [counts + 496], 48000
    CHECK16 126, dword [counts + 500], 60000
    CHECK16 127, dword [counts + 504], 96000
    CHECK16 128, dword [counts + 508], 96000
    CHECK16 129, dword [counts + 512], 96000
    CHECK16 130, dword [counts + 516], 0
    CHECK16 131, dword [counts + 520], 0
    CHECK16 132, dword [counts + 524], 0
    CHECK16 133, dword [counts + 528], 96000
    CHECK16 134, dword [counts + 532], 96000
    CHECK16 135, dword [counts + 536], 96000
    CHECK16 136, dword [counts + 540], 0
    CHECK16 137, dword [counts + 544], 0
    CHECK16 138, dword [counts + 548], 0
    CHECK16 139, dword [counts + 552], 84000
    CHECK16 140, dword [counts + 556], 84000
    CHECK16 141, dword [counts + 560], 96000
    CHECK16 142, dword [counts + 564], 12000
    CHECK16 143, dword [counts + 568], 12000
    CHECK16 144, dword [counts + 572], 0
    CHECK16 145, dword [counts + 576], 84000
    CHECK16 146, dword [counts + 580], 84000
    CHECK16 147, dword [counts + 584], 96000
    CHECK16 148, dword [counts + 588], 12000
    CHECK16 149, dword [counts + 592], 12000
    CHECK16 150, dword [counts + 596], 0
    CHECK16 151, dword [counts + 600], 36000
    CHECK16 152, dword [counts + 604], 36000
    CHECK16 153, dword [counts + 608], 0
    CHECK16 154, dword [counts + 612], 60000
    CHECK16 155, dword [counts + 616], 60000
    CHECK16 156, dword [counts + 620], 96000
    CHECK16 157, dword [counts + 624], 36000
    CHECK16 158, dword [counts + 628], 36000
    CHECK16 159, dword [counts + 632], 0
    CHECK16 160, dword [counts + 636], 60000
    CHECK16 161, dword [counts + 640], 60000
    CHECK16 162, dword [counts + 644], 96000
    CHECK16 163, dword [counts + 648], 24000
    CHECK16 164, dword [counts + 652], 24000
    CHECK16 165, dword [counts + 656], 0
    CHECK16 166, dword [counts + 660], 72000
    CHECK16 167, dword [counts + 664], 72000
    CHECK16 168, dword [counts + 668], 96000
    CHECK16 169, dword [counts + 672], 96000
    CHECK16 170, dword [counts + 676], 96000
    CHECK16 171, dword [counts + 680], 96000
    CHECK16 172, dword [counts + 684], 0
    CHECK16 173, dword [counts + 688], 0
    CHECK16 174, dword [counts + 692], 0
    CHECK16 175, dword [counts + 696], 96000
    CHECK16 176, dword [counts + 700], 96000
    CHECK16 177, dword [counts + 704], 96000
    CHECK16 178, dword [counts + 708], 0
    CHECK16 179, dword [counts + 712], 0
    CHECK16 180, dword [counts + 716], 0
    CHECK16 181, dword [counts + 720], 60000
    CHECK16 182, dword [counts + 724], 60000
    CHECK16 183, dword [counts + 728], 96000
    CHECK16 184, dword [counts + 732], 36000
    CHECK16 185, dword [counts + 736], 36000
    CHECK16 186, dword [counts + 740], 0
    CHECK16 187, dword [counts + 744], 60000
    CHECK16 188, dword [counts + 748], 60000
    CHECK16 189, dword [counts + 752], 96000
    CHECK16 190, dword [counts + 756], 36000
    CHECK16 191, dword [counts + 760], 36000
    CHECK16 192, dword [counts + 764], 0
    CHECK16 193, dword [counts + 768], 48000
    CHECK16 194, dword [counts + 772], 48000
    CHECK16 195, dword [counts + 776], 36000
    CHECK16 196, dword [counts + 780], 48000
    CHECK16 197, dword [counts + 784], 48000
    CHECK16 198, dword [counts + 788], 60000
    CHECK16 199, dword [counts + 792], 48000
    CHECK16 200, dword [counts + 796], 48000
    CHECK16 201, dword [counts + 800], 36000
    CHECK16 202, dword [counts + 804], 48000
    CHECK16 203, dword [counts + 808], 48000
    CHECK16 204, dword [counts + 812], 60000
    CHECK16 205, dword [counts + 816], 12000
    CHECK16 206, dword [counts + 820], 12000
    CHECK16 207, dword [counts + 824], 36000
    CHECK16 208, dword [counts + 828], 84000
    CHECK16 209, dword [counts + 832], 84000
    CHECK16 210, dword [counts + 836], 60000
    PASS

align 4096
body_sub:
    mov [scratch], ebx
    sub dword [scratch], eax
    jo t0
    add dword [counts + 0], 1
t0:
    mov edx, ebx
    sub edx, eax
    jo t1
    add dword [counts + 4], 1
t1:
    mov edx, ebx
    sub edx, 0x80000001
    jo t2
    add dword [counts + 8], 1
t2:
    mov [scratch], ebx
    sub dword [scratch], eax
    jno t3
    add dword [counts + 12], 1
t3:
    mov edx, ebx
    sub edx, eax
    jno t4
    add dword [counts + 16], 1
t4:
    mov edx, ebx
    sub edx, 0x80000001
    jno t5
    add dword [counts + 20], 1
t5:
    mov [scratch], ebx
    sub dword [scratch], eax
    jb t6
    add dword [counts + 24], 1
t6:
    mov edx, ebx
    sub edx, eax
    jb t7
    add dword [counts + 28], 1
t7:
    mov edx, ebx
    sub edx, 0x80000001
    jb t8
    add dword [counts + 32], 1
t8:
    mov [scratch], ebx
    sub dword [scratch], eax
    jae t9
    add dword [counts + 36], 1
t9:
    mov edx, ebx
    sub edx, eax
    jae t10
    add dword [counts + 40], 1
t10:
    mov edx, ebx
    sub edx, 0x80000001
    jae t11
    add dword [counts + 44], 1
t11:
    mov [scratch], ebx
    sub dword [scratch], eax
    je t12
    add dword [counts + 48], 1
t12:
    mov edx, ebx
    sub edx, eax
    je t13
    add dword [counts + 52], 1
t13:
    mov edx, ebx
    sub edx, 0x80000001
    je t14
    add dword [counts + 56], 1
t14:
    mov [scratch], ebx
    sub dword [scratch], eax
    jne t15
    add dword [counts + 60], 1
t15:
    mov edx, ebx
    sub edx, eax
    jne t16
    add dword [counts + 64], 1
t16:
    mov edx, ebx
    sub edx, 0x80000001
    jne t17
    add dword [counts + 68], 1
t17:
    mov [scratch], ebx
    sub dword [scratch], eax
    jbe t18
    add dword [counts + 72], 1
t18:
    mov edx, ebx
    sub edx, eax
    jbe t19
    add dword [counts + 76], 1
t19:
    mov edx, ebx
    sub edx, 0x80000001
    jbe t20
    add dword [counts + 80], 1
t20:
    mov [scratch], ebx
    sub dword [scratch], eax
    ja t21
    add dword [counts + 84], 1
t21:
    mov edx, ebx
    sub edx, eax
    ja t22
    add dword [counts + 88], 1
t22:
    mov edx, ebx
    sub edx, 0x80000001
    ja t23
    add dword [counts + 92], 1
t23:
    mov [scratch], ebx
    sub dword [scratch], eax
    js t24
    add dword [counts + 96], 1
t24:
    mov edx, ebx
    sub edx, eax
    js t25
    add dword [counts + 100], 1
t25:
    mov edx, ebx
    sub edx, 0x80000001
    js t26
    add dword [counts + 104], 1
t26:
    mov [scratch], ebx
    sub dword [scratch], eax
    jns t27
    add dword [counts + 108], 1
t27:
    mov edx, ebx
    sub edx, eax
    jns t28
    add dword [counts + 112], 1
t28:
    mov edx, ebx
    sub edx, 0x80000001
    jns t29
    add dword [counts + 116], 1
t29:
    mov [scratch], ebx
    sub dword [scratch], eax
    jl t30
    add dword [counts + 120], 1
t30:
    mov edx, ebx
    sub edx, eax
    jl t31
    add dword [counts + 124], 1
t31:
    mov edx, ebx
    sub edx, 0x80000001
    jl t32
    add dword [counts + 128], 1
t32:
    mov [scratch], ebx
    sub dword [scratch], eax
    jge t33
    add dword [counts + 132], 1
t33:
    mov edx, ebx
    sub edx, eax
    jge t34
    add dword [counts + 136], 1
t34:
    mov edx, ebx
    sub edx, 0x80000001
    jge t35
    add dword [counts + 140], 1
t35:
    mov [scratch], ebx
    sub dword [scratch], eax
    jle t36
    add dword [counts + 144], 1
t36:
    mov edx, ebx
    sub edx, eax
    jle t37
    add dword [counts + 148], 1
t37:
    mov edx, ebx
    sub edx, 0x80000001
    jle t38
    add dword [counts + 152], 1
t38:
    mov [scratch], ebx
    sub dword [scratch], eax
    jg t39
    add dword [counts + 156], 1
t39:
    mov edx, ebx
    sub edx, eax
    jg t40
    add dword [counts + 160], 1
t40:
    mov edx, ebx
    sub edx, 0x80000001
    jg t41
    add dword [counts + 164], 1
t41:
    ret

align 4096
body_add:
    mov [scratch], ebx
    add dword [scratch], eax
    jo t42
    add dword [counts + 168], 1
t42:
    mov edx, ebx
    add edx, eax
    jo t43
    add dword [counts + 172], 1
t43:
    mov edx, ebx
    add edx, 0x80000001
    jo t44
    add dword [counts + 176], 1
t44:
    mov [scratch], ebx
    add dword [scratch], eax
    jno t45
    add dword [counts + 180], 1
t45:
    mov edx, ebx
    add edx, eax
    jno t46
    add dword [counts + 184], 1
t46:
    mov edx, ebx
    add edx, 0x80000001
    jno t47
    add dword [counts + 188], 1
t47:
    mov [scratch], ebx
    add dword [scratch], eax
    jb t48
    add dword [counts + 192], 1
t48:
    mov edx, ebx
    add edx, eax
    jb t49
    add dword [counts + 196], 1
t49:
    mov edx, ebx
    add edx, 0x80000001
    jb t50
    add dword [counts + 200], 1
t50:
    mov [scratch], ebx
    add dword [scratch], eax
    jae t51
    add dword [counts + 204], 1
t51:
    mov edx, ebx
    add edx, eax
    jae t52
    add dword [counts + 208], 1
t52:
    mov edx, ebx
    add edx, 0x80000001
    jae t53
    add dword [counts + 212], 1
t53:
    mov [scratch], ebx
    add dword [scratch], eax
    je t54
    add dword [counts + 216], 1
t54:
    mov edx, ebx
    add edx, eax
    je t55
    add dword [counts + 220], 1
t55:
    mov edx, ebx
    add edx, 0x80000001
    je t56
    add dword [counts + 224], 1
t56:
    mov [scratch], ebx
    add dword [scratch], eax
    jne t57
    add dword [counts + 228], 1
t57:
    mov edx, ebx
    add edx, eax
    jne t58
    add dword [counts + 232], 1
t58:
    mov edx, ebx
    add edx, 0x80000001
    jne t59
    add dword [counts + 236], 1
t59:
    mov [scratch], ebx
    add dword [scratch], eax
    jbe t60
    add dword [counts + 240], 1
t60:
    mov edx, ebx
    add edx, eax
    jbe t61
    add dword [counts + 244], 1
t61:
    mov edx, ebx
    add edx, 0x80000001
    jbe t62
    add dword [counts + 248], 1
t62:
    mov [scratch], ebx
    add dword [scratch], eax
    ja t63
    add dword [counts + 252], 1
t63:
    mov edx, ebx
    add edx, eax
    ja t64
    add dword [counts + 256], 1
t64:
    mov edx, ebx
    add edx, 0x80000001
    ja t65
    add dword [counts + 260], 1
t65:
    mov [scratch], ebx
    add dword [scratch], eax
    js t66
    add dword [counts + 264], 1
t66:
    mov edx, ebx
    add edx, eax
    js t67
    add dword [counts + 268], 1
t67:
    mov edx, ebx
    add edx, 0x80000001
    js t68
    add dword [counts + 272], 1
t68:
    mov [scratch], ebx
    add dword [scratch], eax
    jns t69
    add dword [counts + 276], 1
t69:
    mov edx, ebx
    add edx, eax
    jns t70
    add dword [counts + 280], 1
t70:
    mov edx, ebx
    add edx, 0x80000001
    jns t71
    add dword [counts + 284], 1
t71:
    mov [scratch], ebx
    add dword [scratch], eax
    jl t72
    add dword [counts + 288], 1
t72:
    mov edx, ebx
    add edx, eax
    jl t73
    add dword [counts + 292], 1
t73:
    mov edx, ebx
    add edx, 0x80000001
    jl t74
    add dword [counts + 296], 1
t74:
    mov [scratch], ebx
    add dword [scratch], eax
    jge t75
    add dword [counts + 300], 1
t75:
    mov edx, ebx
    add edx, eax
    jge t76
    add dword [counts + 304], 1
t76:
    mov edx, ebx
    add edx, 0x80000001
    jge t77
    add dword [counts + 308], 1
t77:
    mov [scratch], ebx
    add dword [scratch], eax
    jle t78
    add dword [counts + 312], 1
t78:
    mov edx, ebx
    add edx, eax
    jle t79
    add dword [counts + 316], 1
t79:
    mov edx, ebx
    add edx, 0x80000001
    jle t80
    add dword [counts + 320], 1
t80:
    mov [scratch], ebx
    add dword [scratch], eax
    jg t81
    add dword [counts + 324], 1
t81:
    mov edx, ebx
    add edx, eax
    jg t82
    add dword [counts + 328], 1
t82:
    mov edx, ebx
    add edx, 0x80000001
    jg t83
    add dword [counts + 332], 1
t83:
    ret

align 4096
body_and:
    mov [scratch], ebx
    and dword [scratch], eax
    jo t84
    add dword [counts + 336], 1
t84:
    mov edx, ebx
    and edx, eax
    jo t85
    add dword [counts + 340], 1
t85:
    mov edx, ebx
    and edx, 0x80000001
    jo t86
    add dword [counts + 344], 1
t86:
    mov [scratch], ebx
    and dword [scratch], eax
    jno t87
    add dword [counts + 348], 1
t87:
    mov edx, ebx
    and edx, eax
    jno t88
    add dword [counts + 352], 1
t88:
    mov edx, ebx
    and edx, 0x80000001
    jno t89
    add dword [counts + 356], 1
t89:
    mov [scratch], ebx
    and dword [scratch], eax
    jb t90
    add dword [counts + 360], 1
t90:
    mov edx, ebx
    and edx, eax
    jb t91
    add dword [counts + 364], 1
t91:
    mov edx, ebx
    and edx, 0x80000001
    jb t92
    add dword [counts + 368], 1
t92:
    mov [scratch], ebx
    and dword [scratch], eax
    jae t93
    add dword [counts + 372], 1
t93:
    mov edx, ebx
    and edx, eax
    jae t94
    add dword [counts + 376], 1
t94:
    mov edx, ebx
    and edx, 0x80000001
    jae t95
    add dword [counts + 380], 1
t95:
    mov [scratch], ebx
    and dword [scratch], eax
    je t96
    add dword [counts + 384], 1
t96:
    mov edx, ebx
    and edx, eax
    je t97
    add dword [counts + 388], 1
t97:
    mov edx, ebx
    and edx, 0x80000001
    je t98
    add dword [counts + 392], 1
t98:
    mov [scratch], ebx
    and dword [scratch], eax
    jne t99
    add dword [counts + 396], 1
t99:
    mov edx, ebx
    and edx, eax
    jne t100
    add dword [counts + 400], 1
t100:
    mov edx, ebx
    and edx, 0x80000001
    jne t101
    add dword [counts + 404], 1
t101:
    mov [scratch], ebx
    and dword [scratch], eax
    jbe t102
    add dword [counts + 408], 1
t102:
    mov edx, ebx
    and edx, eax
    jbe t103
    add dword [counts + 412], 1
t103:
    mov edx, ebx
    and edx, 0x80000001
    jbe t104
    add dword [counts + 416], 1
t104:
    mov [scratch], ebx
    and dword [scratch], eax
    ja t105
    add dword [counts + 420], 1
t105:
    mov edx, ebx
    and edx, eax
    ja t106
    add dword [counts + 424], 1
t106:
    mov edx, ebx
    and edx, 0x80000001
    ja t107
    add dword [counts + 428], 1
t107:
    mov [scratch], ebx
    and dword [scratch], eax
    js t108
    add dword [counts + 432], 1
t108:
    mov edx, ebx
    and edx, eax
    js t109
    add dword [counts + 436], 1
t109:
    mov edx, ebx
    and edx, 0x80000001
    js t110
    add dword [counts + 440], 1
t110:
    mov [scratch], ebx
    and dword [scratch], eax
    jns t111
    add dword [counts + 444], 1
t111:
    mov edx, ebx
    and edx, eax
    jns t112
    add dword [counts + 448], 1
t112:
    mov edx, ebx
    and edx, 0x80000001
    jns t113
    add dword [counts + 452], 1
t113:
    mov [scratch], ebx
    and dword [scratch], eax
    jl t114
    add dword [counts + 456], 1
t114:
    mov edx, ebx
    and edx, eax
    jl t115
    add dword [counts + 460], 1
t115:
    mov edx, ebx
    and edx, 0x80000001
    jl t116
    add dword [counts + 464], 1
t116:
    mov [scratch], ebx
    and dword [scratch], eax
    jge t117
    add dword [counts + 468], 1
t117:
    mov edx, ebx
    and edx, eax
    jge t118
    add dword [counts + 472], 1
t118:
    mov edx, ebx
    and edx, 0x80000001
    jge t119
    add dword [counts + 476], 1
t119:
    mov [scratch], ebx
    and dword [scratch], eax
    jle t120
    add dword [counts + 480], 1
t120:
    mov edx, ebx
    and edx, eax
    jle t121
    add dword [counts + 484], 1
t121:
    mov edx, ebx
    and edx, 0x80000001
    jle t122
    add dword [counts + 488], 1
t122:
    mov [scratch], ebx
    and dword [scratch], eax
    jg t123
    add dword [counts + 492], 1
t123:
    mov edx, ebx
    and edx, eax
    jg t124
    add dword [counts + 496], 1
t124:
    mov edx, ebx
    and edx, 0x80000001
    jg t125
    add dword [counts + 500], 1
t125:
    ret

align 4096
body_or:
    mov [scratch], ebx
    or dword [scratch], eax
    jo t126
    add dword [counts + 504], 1
t126:
    mov edx, ebx
    or edx, eax
    jo t127
    add dword [counts + 508], 1
t127:
    mov edx, ebx
    or edx, 0x80000001
    jo t128
    add dword [counts + 512], 1
t128:
    mov [scratch], ebx
    or dword [scratch], eax
    jno t129
    add dword [counts + 516], 1
t129:
    mov edx, ebx
    or edx, eax
    jno t130
    add dword [counts + 520], 1
t130:
    mov edx, ebx
    or edx, 0x80000001
    jno t131
    add dword [counts + 524], 1
t131:
    mov [scratch], ebx
    or dword [scratch], eax
    jb t132
    add dword [counts + 528], 1
t132:
    mov edx, ebx
    or edx, eax
    jb t133
    add dword [counts + 532], 1
t133:
    mov edx, ebx
    or edx, 0x80000001
    jb t134
    add dword [counts + 536], 1
t134:
    mov [scratch], ebx
    or dword [scratch], eax
    jae t135
    add dword [counts + 540], 1
t135:
    mov edx, ebx
    or edx, eax
    jae t136
    add dword [counts + 544], 1
t136:
    mov edx, ebx
    or edx, 0x80000001
    jae t137
    add dword [counts + 548], 1
t137:
    mov [scratch], ebx
    or dword [scratch], eax
    je t138
    add dword [counts + 552], 1
t138:
    mov edx, ebx
    or edx, eax
    je t139
    add dword [counts + 556], 1
t139:
    mov edx, ebx
    or edx, 0x80000001
    je t140
    add dword [counts + 560], 1
t140:
    mov [scratch], ebx
    or dword [scratch], eax
    jne t141
    add dword [counts + 564], 1
t141:
    mov edx, ebx
    or edx, eax
    jne t142
    add dword [counts + 568], 1
t142:
    mov edx, ebx
    or edx, 0x80000001
    jne t143
    add dword [counts + 572], 1
t143:
    mov [scratch], ebx
    or dword [scratch], eax
    jbe t144
    add dword [counts + 576], 1
t144:
    mov edx, ebx
    or edx, eax
    jbe t145
    add dword [counts + 580], 1
t145:
    mov edx, ebx
    or edx, 0x80000001
    jbe t146
    add dword [counts + 584], 1
t146:
    mov [scratch], ebx
    or dword [scratch], eax
    ja t147
    add dword [counts + 588], 1
t147:
    mov edx, ebx
    or edx, eax
    ja t148
    add dword [counts + 592], 1
t148:
    mov edx, ebx
    or edx, 0x80000001
    ja t149
    add dword [counts + 596], 1
t149:
    mov [scratch], ebx
    or dword [scratch], eax
    js t150
    add dword [counts + 600], 1
t150:
    mov edx, ebx
    or edx, eax
    js t151
    add dword [counts + 604], 1
t151:
    mov edx, ebx
    or edx, 0x80000001
    js t152
    add dword [counts + 608], 1
t152:
    mov [scratch], ebx
    or dword [scratch], eax
    jns t153
    add dword [counts + 612], 1
t153:
    mov edx, ebx
    or edx, eax
    jns t154
    add dword [counts + 616], 1
t154:
    mov edx, ebx
    or edx, 0x80000001
    jns t155
    add dword [counts + 620], 1
t155:
    mov [scratch], ebx
    or dword [scratch], eax
    jl t156
    add dword [counts + 624], 1
t156:
    mov edx, ebx
    or edx, eax
    jl t157
    add dword [counts + 628], 1
t157:
    mov edx, ebx
    or edx, 0x80000001
    jl t158
    add dword [counts + 632], 1
t158:
    mov [scratch], ebx
    or dword [scratch], eax
    jge t159
    add dword [counts + 636], 1
t159:
    mov edx, ebx
    or edx, eax
    jge t160
    add dword [counts + 640], 1
t160:
    mov edx, ebx
    or edx, 0x80000001
    jge t161
    add dword [counts + 644], 1
t161:
    mov [scratch], ebx
    or dword [scratch], eax
    jle t162
    add dword [counts + 648], 1
t162:
    mov edx, ebx
    or edx, eax
    jle t163
    add dword [counts + 652], 1
t163:
    mov edx, ebx
    or edx, 0x80000001
    jle t164
    add dword [counts + 656], 1
t164:
    mov [scratch], ebx
    or dword [scratch], eax
    jg t165
    add dword [counts + 660], 1
t165:
    mov edx, ebx
    or edx, eax
    jg t166
    add dword [counts + 664], 1
t166:
    mov edx, ebx
    or edx, 0x80000001
    jg t167
    add dword [counts + 668], 1
t167:
    ret

align 4096
body_xor:
    mov [scratch], ebx
    xor dword [scratch], eax
    jo t168
    add dword [counts + 672], 1
t168:
    mov edx, ebx
    xor edx, eax
    jo t169
    add dword [counts + 676], 1
t169:
    mov edx, ebx
    xor edx, 0x80000001
    jo t170
    add dword [counts + 680], 1
t170:
    mov [scratch], ebx
    xor dword [scratch], eax
    jno t171
    add dword [counts + 684], 1
t171:
    mov edx, ebx
    xor edx, eax
    jno t172
    add dword [counts + 688], 1
t172:
    mov edx, ebx
    xor edx, 0x80000001
    jno t173
    add dword [counts + 692], 1
t173:
    mov [scratch], ebx
    xor dword [scratch], eax
    jb t174
    add dword [counts + 696], 1
t174:
    mov edx, ebx
    xor edx, eax
    jb t175
    add dword [counts + 700], 1
t175:
    mov edx, ebx
    xor edx, 0x80000001
    jb t176
    add dword [counts + 704], 1
t176:
    mov [scratch], ebx
    xor dword [scratch], eax
    jae t177
    add dword [counts + 708], 1
t177:
    mov edx, ebx
    xor edx, eax
    jae t178
    add dword [counts + 712], 1
t178:
    mov edx, ebx
    xor edx, 0x80000001
    jae t179
    add dword [counts + 716], 1
t179:
    mov [scratch], ebx
    xor dword [scratch], eax
    je t180
    add dword [counts + 720], 1
t180:
    mov edx, ebx
    xor edx, eax
    je t181
    add dword [counts + 724], 1
t181:
    mov edx, ebx
    xor edx, 0x80000001
    je t182
    add dword [counts + 728], 1
t182:
    mov [scratch], ebx
    xor dword [scratch], eax
    jne t183
    add dword [counts + 732], 1
t183:
    mov edx, ebx
    xor edx, eax
    jne t184
    add dword [counts + 736], 1
t184:
    mov edx, ebx
    xor edx, 0x80000001
    jne t185
    add dword [counts + 740], 1
t185:
    mov [scratch], ebx
    xor dword [scratch], eax
    jbe t186
    add dword [counts + 744], 1
t186:
    mov edx, ebx
    xor edx, eax
    jbe t187
    add dword [counts + 748], 1
t187:
    mov edx, ebx
    xor edx, 0x80000001
    jbe t188
    add dword [counts + 752], 1
t188:
    mov [scratch], ebx
    xor dword [scratch], eax
    ja t189
    add dword [counts + 756], 1
t189:
    mov edx, ebx
    xor edx, eax
    ja t190
    add dword [counts + 760], 1
t190:
    mov edx, ebx
    xor edx, 0x80000001
    ja t191
    add dword [counts + 764], 1
t191:
    mov [scratch], ebx
    xor dword [scratch], eax
    js t192
    add dword [counts + 768], 1
t192:
    mov edx, ebx
    xor edx, eax
    js t193
    add dword [counts + 772], 1
t193:
    mov edx, ebx
    xor edx, 0x80000001
    js t194
    add dword [counts + 776], 1
t194:
    mov [scratch], ebx
    xor dword [scratch], eax
    jns t195
    add dword [counts + 780], 1
t195:
    mov edx, ebx
    xor edx, eax
    jns t196
    add dword [counts + 784], 1
t196:
    mov edx, ebx
    xor edx, 0x80000001
    jns t197
    add dword [counts + 788], 1
t197:
    mov [scratch], ebx
    xor dword [scratch], eax
    jl t198
    add dword [counts + 792], 1
t198:
    mov edx, ebx
    xor edx, eax
    jl t199
    add dword [counts + 796], 1
t199:
    mov edx, ebx
    xor edx, 0x80000001
    jl t200
    add dword [counts + 800], 1
t200:
    mov [scratch], ebx
    xor dword [scratch], eax
    jge t201
    add dword [counts + 804], 1
t201:
    mov edx, ebx
    xor edx, eax
    jge t202
    add dword [counts + 808], 1
t202:
    mov edx, ebx
    xor edx, 0x80000001
    jge t203
    add dword [counts + 812], 1
t203:
    mov [scratch], ebx
    xor dword [scratch], eax
    jle t204
    add dword [counts + 816], 1
t204:
    mov edx, ebx
    xor edx, eax
    jle t205
    add dword [counts + 820], 1
t205:
    mov edx, ebx
    xor edx, 0x80000001
    jle t206
    add dword [counts + 824], 1
t206:
    mov [scratch], ebx
    xor dword [scratch], eax
    jg t207
    add dword [counts + 828], 1
t207:
    mov edx, ebx
    xor edx, eax
    jg t208
    add dword [counts + 832], 1
t208:
    mov edx, ebx
    xor edx, 0x80000001
    jg t209
    add dword [counts + 836], 1
t209:
    ret

align 4096
pairs_a: dd 0x0, 0x7fffffff, 0x80000000, 0x1, 0xffffffff, 0x5, 0x12345678, 0x80000000
pairs_b: dd 0x0, 0x1, 0x1, 0x80000000, 0x1, 0x5, 0x87654321, 0x80000000
scratch: dd 0
counts: times 210 dd 0
