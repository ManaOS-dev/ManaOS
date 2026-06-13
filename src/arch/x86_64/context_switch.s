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

.def switch_to_user_mode
.scl 2
.type 32
.endef
.globl switch_to_user_mode

# Saves the current kernel/task context at rcx, then restores the UserTrapFrame
# pointed to by rdx and enters Ring 3 with iretq.
switch_to_user_mode:
    mov qword ptr [rcx + 0], rsp
    mov qword ptr [rcx + 8], r15
    mov qword ptr [rcx + 16], r14
    mov qword ptr [rcx + 24], r13
    mov qword ptr [rcx + 32], r12
    mov qword ptr [rcx + 40], rbx
    mov qword ptr [rcx + 48], rbp
    pushfq
    pop qword ptr [rcx + 56]

    mov rbx, rdx
    mov rax, qword ptr [rbx + 32]
    push rax
    mov rax, qword ptr [rbx + 24]
    push rax
    mov rax, qword ptr [rbx + 16]
    push rax
    mov rax, qword ptr [rbx + 8]
    push rax
    mov rax, qword ptr [rbx + 0]
    push rax
    mov rax, qword ptr [rbx + 40]
    mov rcx, qword ptr [rbx + 56]
    mov rdx, qword ptr [rbx + 64]
    mov rsi, qword ptr [rbx + 72]
    mov rdi, qword ptr [rbx + 80]
    mov rbp, qword ptr [rbx + 88]
    mov r8, qword ptr [rbx + 96]
    mov r9, qword ptr [rbx + 104]
    mov r10, qword ptr [rbx + 112]
    mov r11, qword ptr [rbx + 120]
    mov r12, qword ptr [rbx + 128]
    mov r13, qword ptr [rbx + 136]
    mov r14, qword ptr [rbx + 144]
    mov r15, qword ptr [rbx + 152]
    mov rbx, qword ptr [rbx + 48]
    iretq

.def enter_user_mode_returnable
.scl 2
.type 32
.endef
.globl enter_user_mode_returnable

# UserTrapFrame layout, kept in sync with kernel::task::context:
#   +0   user rip
#   +8   user cs
#   +16  user rflags
#   +24  user rsp
#   +32  user ss
#   +40  rax
#   +48  rbx
#   +56  rcx
#   +64  rdx
#   +72  rsi
#   +80  rdi
#   +88  rbp
#   +96  r8
#   +104 r9
#   +112 r10
#   +120 r11
#   +128 r12
#   +136 r13
#   +144 r14
#   +152 r15
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
    call set_user_return_stack
    add rsp, 32

    mov rax, qword ptr [rbx + 32]
    push rax
    mov rax, qword ptr [rbx + 24]
    push rax
    mov rax, qword ptr [rbx + 16]
    push rax
    mov rax, qword ptr [rbx + 8]
    push rax
    mov rax, qword ptr [rbx + 0]
    push rax
    mov rax, qword ptr [rbx + 40]
    mov rcx, qword ptr [rbx + 56]
    mov rdx, qword ptr [rbx + 64]
    mov rsi, qword ptr [rbx + 72]
    mov rdi, qword ptr [rbx + 80]
    mov rbp, qword ptr [rbx + 88]
    mov r8, qword ptr [rbx + 96]
    mov r9, qword ptr [rbx + 104]
    mov r10, qword ptr [rbx + 112]
    mov r11, qword ptr [rbx + 120]
    mov r12, qword ptr [rbx + 128]
    mov r13, qword ptr [rbx + 136]
    mov r14, qword ptr [rbx + 144]
    mov r15, qword ptr [rbx + 152]
    mov rbx, qword ptr [rbx + 48]
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
