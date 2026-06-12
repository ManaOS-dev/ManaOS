.text
.def context_switch
.scl 2
.type 32
.endef
.globl context_switch

# TaskContext layout, kept in sync with kernel::task::context:
#   +0  rsp
#   +8  r15
#   +16 r14
#   +24 r13
#   +32 r12
#   +40 rbx
#   +48 rbp
#   +56 rflags
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

# UserTaskContext layout, kept in sync with kernel::task::context:
#   +0  user rip
#   +8  user cs
#   +16 user rflags
#   +24 user rsp
#   +32 user ss
#   +40 argc -> rdi
#   +48 argv -> rsi
#   +56 envp -> rdx
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
    mov rdi, qword ptr [rcx + 40]
    mov rsi, qword ptr [rcx + 48]
    mov rdx, qword ptr [rcx + 56]
    iretq

.def enter_user_mode_returnable
.scl 2
.type 32
.endef
.globl enter_user_mode_returnable

# Returnable entry consumes the same UserTaskContext layout as enter_user_mode.
enter_user_mode_returnable:
    push r15
    push r14
    push r13
    push r12
    push rsi
    push rdi
    push rbp
    push rbx
    mov rbx, rcx
    lea rax, [rip + enter_user_mode_returnable_exit]
    push rax
    mov rcx, rsp
    sub rsp, 32
    call set_user_exit_return_stack
    add rsp, 32
    mov rcx, rbx

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
    mov rdi, qword ptr [rcx + 40]
    mov rsi, qword ptr [rcx + 48]
    mov rdx, qword ptr [rcx + 56]
    iretq

enter_user_mode_returnable_exit:
    sti
    pop rbx
    pop rbp
    pop rdi
    pop rsi
    pop r12
    pop r13
    pop r14
    pop r15
    ret
