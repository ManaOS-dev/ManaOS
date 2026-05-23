.text
.def context_switch
.scl 2
.type 32
.endef
.globl context_switch

context_switch:
    mov qword ptr [rcx + 0], rsp
    mov qword ptr [rcx + 8], r15
    mov qword ptr [rcx + 16], r14
    mov qword ptr [rcx + 24], r13
    mov qword ptr [rcx + 32], r12
    mov qword ptr [rcx + 40], rbx
    mov qword ptr [rcx + 48], rbp
    pushfq
    pop qword ptr [rcx + 56]

    mov rsp, qword ptr [rdx + 0]
    mov r15, qword ptr [rdx + 8]
    mov r14, qword ptr [rdx + 16]
    mov r13, qword ptr [rdx + 24]
    mov r12, qword ptr [rdx + 32]
    mov rbx, qword ptr [rdx + 40]
    mov rbp, qword ptr [rdx + 48]
    push qword ptr [rdx + 56]
    popfq
    ret

.def enter_user_mode
.scl 2
.type 32
.endef
.globl enter_user_mode

enter_user_mode:
    mov rax, qword ptr [rcx + 32]
    push rax
    mov rax, qword ptr [rcx + 24]
    push rax
    mov rax, qword ptr [rcx + 16]
    push rax
    mov rax, qword ptr [rcx + 8]
    push rax
    mov rax, qword ptr [rcx + 0]
    push rax
    iretq
