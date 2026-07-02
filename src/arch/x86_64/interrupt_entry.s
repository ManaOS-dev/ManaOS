.text
.def timer_interrupt_handler_entry
.scl 2
.type 32
.endef
.globl timer_interrupt_handler_entry

# RawTimerInterruptFrame layout, kept in sync with arch::x86_64::interrupt_descriptor_table:
#   +0   interrupted rip
#   +8   interrupted cs
#   +16  interrupted rflags
#   +24  interrupted user rsp, or 0 for kernel frames
#   +32  interrupted user ss, or 0 for kernel frames
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
timer_interrupt_handler_entry:
    # Rust and the x86_64 C ABI require DF=0 before calling Rust code.
    cld
    sub rsp, 160
    mov qword ptr [rsp + 40], rax
    mov qword ptr [rsp + 48], rbx
    mov qword ptr [rsp + 56], rcx
    mov qword ptr [rsp + 64], rdx
    mov qword ptr [rsp + 72], rsi
    mov qword ptr [rsp + 80], rdi
    mov qword ptr [rsp + 88], rbp
    mov qword ptr [rsp + 96], r8
    mov qword ptr [rsp + 104], r9
    mov qword ptr [rsp + 112], r10
    mov qword ptr [rsp + 120], r11
    mov qword ptr [rsp + 128], r12
    mov qword ptr [rsp + 136], r13
    mov qword ptr [rsp + 144], r14
    mov qword ptr [rsp + 152], r15

    mov rax, qword ptr [rsp + 160]
    mov qword ptr [rsp + 0], rax
    mov rax, qword ptr [rsp + 168]
    mov qword ptr [rsp + 8], rax
    mov rax, qword ptr [rsp + 176]
    mov qword ptr [rsp + 16], rax
    mov qword ptr [rsp + 24], 0
    mov qword ptr [rsp + 32], 0

    mov rax, qword ptr [rsp + 168]
    and rax, 3
    cmp rax, 3
    jne 1f
    mov rax, qword ptr [rsp + 184]
    mov qword ptr [rsp + 24], rax
    mov rax, qword ptr [rsp + 192]
    mov qword ptr [rsp + 32], rax

1:
    mov rbx, rsp
    mov rcx, rbx
    test rsp, 15
    jz 2f
    sub rsp, 8
2:
    sub rsp, 32
    call push_timer_interrupt_frame
    mov rsp, rbx

    mov rax, qword ptr [rsp + 40]
    mov rbx, qword ptr [rsp + 48]
    mov rcx, qword ptr [rsp + 56]
    mov rdx, qword ptr [rsp + 64]
    mov rsi, qword ptr [rsp + 72]
    mov rdi, qword ptr [rsp + 80]
    mov rbp, qword ptr [rsp + 88]
    mov r8, qword ptr [rsp + 96]
    mov r9, qword ptr [rsp + 104]
    mov r10, qword ptr [rsp + 112]
    mov r11, qword ptr [rsp + 120]
    mov r12, qword ptr [rsp + 128]
    mov r13, qword ptr [rsp + 136]
    mov r14, qword ptr [rsp + 144]
    mov r15, qword ptr [rsp + 152]
    add rsp, 160
    iretq
