extern crate rand;

use rand::random;

/// The default CPU clock, in Hz.
const CPU_CLOCK: u32 = 600;
/// The timers clock, in Hz.
const TIMERS_CLOCK: u32 = 60;

/// The index of the register used for the 'carry flag'.
/// VF is used according to the CHIP 8 specifications.
const FLAG: usize = 15;
/// The size of the stack.
const STACK_SIZE: usize = 16;
/// The size of the register.
const REGISTER_SIZE: usize = 16;
/// Program always loads at this address (512).
const PROGRAM_START: usize = 0x200;
/// Machine memory size.
const MEMORY_SIZE: usize = 4096;

/// Display height.
const DISPLAY_HEIGHT: usize = 64;
/// Display width.
const DISPLAY_WIDTH: usize = 32;


/// Chip8 font set.
static FONT_SET: [u8; 80] = [
    0xF0, 0x90, 0x90, 0x90, 0xF0, // 0
    0x20, 0x60, 0x20, 0x20, 0x70, // 1
    0xF0, 0x10, 0xF0, 0x80, 0xF0, // 2
    0xF0, 0x10, 0xF0, 0x10, 0xF0, // 3
    0x90, 0x90, 0xF0, 0x10, 0x10, // 4
    0xF0, 0x80, 0xF0, 0x10, 0xF0, // 5
    0xF0, 0x80, 0xF0, 0x90, 0xF0, // 6
    0xF0, 0x10, 0x20, 0x40, 0x40, // 7
    0xF0, 0x90, 0xF0, 0x90, 0xF0, // 8
    0xF0, 0x90, 0xF0, 0x10, 0xF0, // 9
    0xF0, 0x90, 0xF0, 0x90, 0x90, // A
    0xE0, 0x90, 0xE0, 0x90, 0xE0, // B
    0xF0, 0x80, 0x80, 0x80, 0xF0, // C
    0xE0, 0x90, 0x90, 0x90, 0xE0, // D
    0xF0, 0x80, 0xF0, 0x80, 0xF0, // E
    0xF0, 0x80, 0xF0, 0x80, 0x80  // F
];


macro_rules! opcode_not_implemented {
    ($op: expr, $pc: expr) => (
        panic!("{:0>4X} opcode not implemented at {:05X}", $op as usize, $pc);
    )
}


struct Display {
    screen: [u8; DISPLAY_HEIGHT * DISPLAY_WIDTH],
}

impl Display {
    fn new() -> Self {
        Display {
            screen: [0; DISPLAY_HEIGHT * DISPLAY_WIDTH]
        }
    }

    /// Get coordinate x,y in one dimensional linear space.
    fn get_coord(x: usize, y: usize) -> usize {
        y * DISPLAY_WIDTH + x
    }
}

/// CHIP-8 machine struct.
struct Chip8 {
    /// Index register
    i: usize,
    /// Program counter
    pc: usize,
    /// Registers refer to as V0 to VF where VF is used primarily for carry
    v: [u8; REGISTER_SIZE],
    /// Stack
    stack: Vec<u16>,
    /// Stack pointer
    sp: usize,
    /// Machine memory
    memory: [u8; MEMORY_SIZE],
    /// Delay timer
    delay_timer: u8,
    /// Sound timer
    sound_timer: u8,
    /// Wait for key press
    wait_for_key: (bool, u8),
    /// Original implementation requires VY to be shifted instead of VX.
    /// Though many many ROM assume otherwise.
    shift_vy: bool,
}

impl Chip8 {
    fn new() -> Self {
        Chip8 {
            i: 0, 
            pc: PROGRAM_START,
            sp: 0,
            v: [0; REGISTER_SIZE],
            stack: Vec::<u16>::new(),
            memory: [0; MEMORY_SIZE],
            delay_timer: 0,
            sound_timer: 0,
            wait_for_key: (false, 0),
            shift_vy: false,
        }
    }

    /// Reinitialize the machine whilst keeping the program inside the memory.
    fn reset(&mut self) {
        self.v = [0; REGISTER_SIZE];
        self.stack.clear();
        self.pc = PROGRAM_START;
        self.i = 0;
        self.sp = 0;
    }

    fn execute_cycle(&mut self) {
        let ops = self.get_opcode();
        self.check_opcode(ops);
    }

    /// Fetches 2 bytes 
    fn get_opcode(&self) -> u16 {
        (self.memory[self.pc] as u16) << 8 | (self.memory[self.pc+1] as u16)
    }

    /// Checks the given opcode and execute an operation.
    fn check_opcode(&mut self, ops: u16) {
        // Set the opcode tuples in the following pattern:
        // 0xABCD
        let op_tuple = (
            ((ops & 0xF000) >> 12) as u8,
            ((ops & 0x0F00) >> 8) as u8,
            ((ops & 0x00F0) >> 4) as u8,
            (ops & 0x000F) as u8,
        );

        // Match the opcode
        // TODO: refactor it to make it easier to see
        match op_tuple {
            // 0x0NNN Execute machine language subroutine at address NNN. Ignore.
            // Inclement program counter by two since every instruction is two bytes long.
            // With an exception for jump and subroutine call.
            (0x0, 0x0, 0xE, 0x0) => self.cls(),
            (0x0, 0x0, 0xE, 0xE) => self.reset(),
            (0x1, _, _, _) => self.jump_addr(ops & 0x0FFF),
            (0x2, _, _, _) => self.call_sub(ops & 0x0FFF),
            (0x3, x, _, _) => self.se_vx(x, (ops & 0x00FF) as u8),
            (0x4, x, _, _) => self.sne_vx(x, (ops & 0x00FF) as u8),
            (0x5, x, y, 0x0) => self.se_vx_vy(x, y),
            (0x6, x, _, _) => self.set_reg_vn(x, (ops & 0x00FF) as u8),
            (0x7, x, _, _) => {
                // Adds the value NN to register VX.
                let vx = self.read_reg_vn(x);
                self.set_reg_vn(x, vx.wrapping_add((ops & 0x00FF) as u8));
            },
            (0x8, x, y, 0x0) => {
                // Stores the value of register VY in register VX.
                let vy = self.read_reg_vn(y);
                self.set_reg_vn(x, vy);
            },
            (0x8, x, y, 0x1) => self.or_vx_vy(x, y),
            (0x8, x, y, 0x2) => self.and_vx_vy(x, y),
            (0x8, x, y, 0x3) => self.xor_vx_vy(x, y),
            (0x8, x, y, 0x4) => self.add_vx_vy(x, y),
            (0x8, x, y, 0x5) => self.sub_vx_vy(x, y),
            (0x8, x, y, 0x6) => self.rshft_vx_vy(x, y),
            (0x8, x, y, 0x7) => self.subn_vx_vy(x, y),
            (0x8, x, y, 0xE) => self.lshft_vx_vy(x, y),
            (0x9, x, y, 0x0) => self.skip_ne_vx_vy(x, y),
            (0xA, _, _, _) => self.set_i_addr(ops & 0x0FFF),
            (0xB, _, _, _) => {
                // Jumps to address NNN + V0.
                let v0 = self.read_reg_vn(0) as u16;
                self.jump_addr(ops & 0x0FFF + v0);
            },
            (0xC, x, _, _) => self.rnd_vx_nn(x, (ops & 0x00FF) as u8),
            (0xD, x, y, n) => self.draw_vx_vy(x, y, n),
            (0xE, x, 0x9, 0xE) => self.skip_vx(x),
            (0xE, x, 0xA, 0x1) => self.skipn_vx(x),
            (0xF, x, 0x0, 0x7) => self.set_delay(x),
            (0xF, x, 0x0, 0xA) => self.wait_vx(x),
            (0xF, x, 0x1, 0x5) => self.set_vx_delay(x),
            (0xF, x, 0x1, 0x8) => self.set_vx_sound(x),
            (0xF, x, 0x1, 0xE) => self.add_vx_to_i(x),
            (0xF, x, 0x2, 0x9) => self.set_i_sprite(x),
            (0xF, x, 0x3, 0x3) => self.set_bcd_vx(x),
            (0xF, x, 0x5, 0x5) => self.set_mem_regs(x),
            (0xF, x, 0x6, 0x5) => self.fill_regs_mem(x),
            _ => opcode_not_implemented!(ops, self.pc),
        }
        
    }

    /// Set V at index N to a specific value.
    fn set_reg_vn(&mut self, n: u8, val: u8) {
        self.v[n as usize] = val;
        self.pc += 2;
    }

    /// Read a value of V at index N.
    fn read_reg_vn(&mut self, n: u8) -> u8 {
        self.v[n as usize]
    }

    /// Clears the display
    fn cls(&mut self) {
        // TODO
        self.pc += 2;
    }

    /// Returns from the subroutine, by setting the program counter
    /// to the address from the top of stack.
    fn ret(&mut self) {
        let addr = self.stack.pop().expect("Stored address");
        self.jump_addr(addr);
        //self.pc += 2;
    }

    /// Jumps program counter to a specified address
    fn jump_addr(&mut self, addr: u16) {
        self.pc = addr as usize;
    }

    /// Calls a subroutine by pushing the current PC to the stack,
    /// then jumps to the given address.
    fn call_sub(&mut self, addr: u16) {
        self.stack.push(self.pc as u16);
        self.jump_addr(addr);
    }

    /// Skips the following instruction if the value of register VX equals NN.
    fn se_vx(&mut self, x: u8, nn: u8) {
        self.pc += if self.v[x as usize] == nn {4} else {2};
    }

    /// Skips the following instruction if the value of register VX is not equal to NN.
    fn sne_vx(&mut self, x: u8, nn: u8) {
        self.pc += if self.v[x as usize] != nn {4} else {2};
    }

    /// Skips the following instruction if the value of 
    /// register VX is equal to the value of register VY.
    fn se_vx_vy(&mut self, x: u8, y: u8) {
        self.pc += if self.v[x as usize] == self.v[y as usize] {4} else {2};
    }

    fn or_vx_vy(&mut self, x: u8, y: u8) {
        self.v[x as usize] |= self.v[y as usize];
        self.pc += 2;
    }

    fn and_vx_vy(&mut self, x: u8, y: u8) {
        self.v[x as usize] &= self.v[y as usize];
        self.pc += 2;
    }

    fn xor_vx_vy(&mut self, x: u8, y: u8) {
        self.v[x as usize] ^= self.v[y as usize];
        self.pc += 2;
    }

    fn add_vx_vy(&mut self, x: u8, y: u8) {
        let sum: u16 = self.v[x as usize] as u16 + self.v[y as usize] as u16;
        self.v[x as usize] = sum as u8;
        self.v[FLAG] = if sum > 0xFF {0x1} else {0x0};
        self.pc += 2;
    }

    /// Subtract the value of register VY from register VX
    /// Set VF to 00 if a borrow occurs
    /// Set VF to 01 if a borrow does not occur.
    fn sub_vx_vy(&mut self, x: u8, y: u8) {
        let diff: i8 = self.v[x as usize] as i8 - self.v[y as usize] as i8;
        self.v[x as usize] = diff as u8;
        self.v[FLAG] = if diff < 0 {0x1} else {0x0};
        self.pc += 2;
    }

    /// Set register VX to the value of VY minus VX
    /// Set VF to 00 if a borrow occurs
    /// Set VF to 01 if a borrow does not occur.
    fn subn_vx_vy(&mut self, x: u8, y: u8) {
        let diff: i8 = self.v[y as usize] as i8 - self.v[x as usize] as i8;
        self.v[x as usize] = diff as u8;
        self.v[FLAG] = if diff < 0 {0x1} else {0x0};        
        self.pc += 2;
    }

    /// Store the value of register VY shifted right one bit in register VX
    /// Set register VF to the least significant bit prior to the shift.
    fn rshft_vx_vy(&mut self, x: u8, y: u8) {
        let n = if self.shift_vy {y} else {x};
        self.v[FLAG] = self.v[n as usize] & 0x01; // 0000 0001
        self.v[x as usize] = self.v[n as usize] >> 1;
        self.pc += 2;
    }

    /// Store the value of register VY shifted left one bit in register VX
    /// Set register VF to the most significant bit prior to the shift.
    fn lshft_vx_vy(&mut self, x: u8, y: u8) {
        let n = if self.shift_vy {y} else {x};
        self.v[FLAG] = self.v[n as usize] & 0x80; // 1000 0000
        self.v[x as usize] = self.v[n as usize] << 1;
        self.pc += 2;
    }

    /// Skips the following instruction if the value of register VX is not equal 
    /// to the value of register VY.
    fn skip_ne_vx_vy(&mut self, x: u8, y: u8) {
        self.pc += if self.v[x as usize] != self.v[y as usize] {4} else {2};
    }

    /// Stores memory address NNN in register I.
    fn set_i_addr(&mut self, addr: u16) {
        self.i = addr as usize;
        self.pc += 2;
    }

    /// Sets VX to a random number with a mask of NN.
    fn rnd_vx_nn(&mut self, x: u8, nn: u8) {
        self.v[x as usize] = rand::random::<u8>() & nn;
        self.pc += 2;
    }

    /// Draws a sprite at position VX, VY with N bytes of sprite data starting at the address stored in I
    /// Set VF to 01 if any set pixels are changed to unset, and 00 otherwise.
    fn draw_vx_vy(&mut self, x: u8, y: u8, n: u8) {
        let pos_x = self.v[x as usize] as usize;
        let pos_y = self.v[y as usize] as usize;
        let start = self.i;
        let end = self.i + n as usize;
        // TODO: draw stuff
        self.pc += 2;
    }

    /// Skips the following instruction if the key corresponding to the hex value 
    /// currently stored in register VX is pressed.
    fn skip_vx(&mut self, x: u8) {
        // TODO: implement keypad
    }

    /// Skips the following instruction if the key corresponding to the hex value 
    /// currently stored in register VX is not pressed.
    fn skipn_vx(&mut self, x: u8) {
        // TODO: implement keypad
    }

    /// Stores the current value of the delay timer in register VX.
    fn set_delay(&mut self, x: u8) {
        self.v[x as usize] = self.delay_timer;
        self.pc += 2;
    }

    /// Waits for a keypress and store the result in register VX.
    fn wait_vx(&mut self, x: u8) {
        self.wait_for_key = (true, x);
    }

    /// Sets the delay timer to the value of register VX.
    fn set_vx_delay(&mut self, x: u8) {
        self.delay_timer = self.v[x as usize];
        self.pc += 2;
    }

    /// Sets the sound timer to the value of register VX.
    fn set_vx_sound(&mut self, x: u8) {
        self.sound_timer = self.v[x as usize];
        self.pc += 2;
    }

    /// Adds the value stored in register VX to register I.
    fn add_vx_to_i(&mut self, x: u8) {
        self.i += self.v[x as usize] as usize;
        self.pc += 2;
    }

    /// Sets I to the memory address of the sprite data corresponding to the hexadecimal digit 
    /// stored in register VX.
    fn set_i_sprite(&mut self, x: u8) {
        // Multiply by 5 because a sprite has 5 lines, a line equates to one byte.
        self.i = (self.v[x as usize] * 5) as usize;
        self.pc += 2;
    }

    /// Stores the binary-coded decimal equivalent of the value stored in register VX at addresses I, I+1, and I+2
    fn set_bcd_vx(&mut self, x: u8) {
        let vx = self.read_reg_vn(x);
        self.memory[self.i] = vx / 100;
        self.memory[self.i+1] = (vx / 10) % 10;
        self.memory[self.i+2] = (vx % 100) % 10;
        self.pc += 2;
    }

    /// Stores the values of registers V0 to VX inclusive in memory starting at address I
    /// I is set to I + X + 1 after operation.
    fn set_mem_regs(&mut self, x: u8) {
        let x_usize = x as usize;
        for i in 0..(x_usize + 1) {
            self.memory[self.i + i] = self.v[i];
        }
        self.i += x_usize + 1;
        self.pc += 2;
    }

    /// Fills registers V0 to VX inclusive with the values stored in memory starting at address I
    /// I is set to I + X + 1 after operation.
    fn fill_regs_mem(&mut self, x: u8) {
        let x_usize = x as usize;
        for i in 0..(x_usize + 1) {
            self.v[i] = self.memory[self.i + i];
        }
        self.i += x_usize + 1;
        self.pc += 2;
    }
}

fn main() {
    // Setup graphic
    // Setup input
    // Initialize the system
    // Game loop

    println!("Hello, world!");
}
