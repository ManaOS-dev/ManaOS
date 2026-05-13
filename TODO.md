# ManaOS TODO

## 🧱 Phase 1: Memory Management & Foundation
- [x] Memory Map Acquisition & `ExitBootServices`
- [x] Physical Frame Allocator (Bump Allocator)
- [x] Heap Allocator (`linked_list_allocator`)
- [x] Architecture Separation (`arch/` layer established)
- [x] Explicit Paging Setup (Identity Mapping)

## ⚡ Phase 2: Interrupts & Exceptions
- [x] GDT / IDT Setup (with Data Segments)
- [x] Exception Handlers (Page Fault, Double Fault, GPF)
- [x] Mouse Driver (PS/2) with Real-time Cursor, Lock-Free Async Queue & Dirty Rectangles
- [x] Keyboard Driver (PS/2) - Interrupt driven & Lock-Free Async Queue
- [ ] Timer Interrupts (Local APIC)
- [ ] Interrupt Controller (IOAPIC) support

## 🖥 Phase 3: Graphics & Console
- [x] Serial Output (COM1)
- [x] GOP Framebuffer Control
- [x] Font Engine (`ab_glyph`)
- [x] Proper Alpha Blending for Text (Pixel-perfect rounding)
- [x] Double Buffering & Dirty Rectangles Optimization (1000fps ready)
- [x] RDTSC Profiling & Calibration

## 🔄 Phase 4: Process Management
- [ ] Task Structure & Context Switching
- [ ] Cooperative / Preemptive Scheduler
- [ ] Transition to User Space (Ring 3)

## 💾 Phase 5: Filesystem & Storage
- [ ] AHCI Driver Implementation
- [ ] FAT32 Parser & File APIs

## 🚀 Phase 6: Userland
- [ ] ELF Loader
- [ ] System Call API Definitions
- [ ] Shell Implementation
