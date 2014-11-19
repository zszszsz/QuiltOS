use std::io::{Stream, IoResult, IoError};
use std::mem::{transmute, size_of};
use arch::cpu::Port;
use rtl8139::Rtl8139;

pub struct Pci {
  address_port: Port,
  data_port: Port
}



struct PciHeader {
  shared: SharedHeader,
  rest: HeaderType
}

#[repr(packed)]
struct SharedHeader {
  vendor: u16,
  device: u16,
  command: u16,
  status: u16,
  revision: u8,
  prog_if: u8,
  subclass: u8,
  class_code: u8,
  cache_line_size: u8,
  latency_timer: u8,
  header_type: u8,
  bist: u8
}

#[repr(packed)]
struct Header1 {
  base_addresses: [u32, ..6],
  cardbus_pointer: u32,
  subsystem_vendor: u16,
  subsystem: u16,
  expansion_rom_address: u32,
  capabilities_pointer: u8,
  reserved: [u8, ..7],
  interrupt_line: u8,
  interrupt_pin: u8,
  min_grant: u8,
  max_latency: u8
}

#[repr(packed)]
struct Header2;

#[repr(packed)]
struct Header3;

enum HeaderType {
  Basic(Header1),
  Todo,
}

fn read_into<'a, T, S>(slice: &'a [S]) -> Box<T> {
  kassert!(size_of::<S>() * slice.len() == size_of::<T>());
  let ret: Box<T> = unsafe { transmute(slice.as_ptr()) };
  return ret
}

impl Pci {

  pub fn new(address_port: Port, data_port: Port) -> Pci {
    Pci { address_port: address_port, data_port: data_port }
  }
  
  pub fn init(&mut self) {}
  
  fn build_address(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    if (function & 0x03 != 0x00) || (device >= 0x1 << 5) || (function >= 0x1 << 3)  {
      kassert!(false)
      return 0;
    } else {
      return (0x1 as u32 << 31) | (bus as u32 << 16) | (device as u32 << 11) | (function as u32 << 8) | offset as u32;
    }
  }
  
  pub fn read(&mut self, bus: u8, device: u8, function: u8, offset: u8) -> IoResult<u32> {
      let address = Pci::build_address(bus, device, function, offset);
      self.address_port.out_l(address);
      //Port::io_wait();
      let input = self.data_port.in_l();
      Ok(input)
  }
  
  pub fn read_bytes(&mut self, bus: u8, device: u8, start_address: u16, size: u16) -> Vec<u32> {
    kassert!(size % 4 == 0)
    kassert!(start_address % 4 == 0)
    
    let mut v = vec!();
    for i in range(0_u16, size / 4) {
      let (offset, function): (u8, u8) = unsafe { transmute((start_address + i*4) as u16) };
      v.push(self.read(bus, device, function, offset).unwrap());
    }
    v
  }
  
  fn read_as<T>(&mut self, bus: u8, device: u8, start_address: u16) -> Box<T> {
    let v = self.read_bytes(bus, device, start_address, size_of::<T>() as u16);
    let slice = v.as_slice(); 
    let read = read_into(slice);
    return read;
  }
  
  fn read_header(&mut self, bus: u8, device: u8) -> Option<PciHeader> {
    let (vendor, device_id): (u16, u16) = unsafe { transmute(self.read(bus, device, 0, 0).unwrap()) };
    if vendor == 0xffff {
      return None
    }
    
    let shared: SharedHeader = *self.read_as(bus, device, 0);
    let rest = match shared.header_type {
      0x00 => Basic(*self.read_as(bus, device, size_of::<SharedHeader>() as u16)),
      0x01 => Todo,
      0x02 => Todo,
      _ => {
	debug!("weird header")
	return None
      }
    };
    return Some(PciHeader { shared: shared, rest: rest });
  }
  
  pub fn check_devices(&mut self) -> (u32, u32) {
    let mut no_device_count = 0;
    let mut device_count = 0;
    
    let mut io_offset: u32 = 0;
    for bus in range(0, 256u) {
      for device in range(0, 32u) {
	match self.read_header(bus as u8, device as u8) {
	  None => no_device_count += 1,
	  Some(header) => {
	    device_count += 1;
	    let shared = header.shared;
	    debug!("bus #{} found device 0x{:x} -- vendor 0x{:x}", bus, shared.device, shared.vendor)
	    debug!("    class 0x{:x}, subclass 0x{:x}", shared.class_code, shared.subclass)
	    debug!("    header type 0x{:x}", shared.header_type)
	    debug!("    status 0x{:x}, command 0x{:x}", shared.status, shared.command)
	    match header.rest {
	      Basic(next) => {
		for &addr in next.base_addresses.iter() {
		  debug!("        base_address: 0x{:x}", addr)
		}
		if (shared.vendor == 0x10ec) && (shared.device == 0x8139) {
		  debug!("found rtl8139!")
		  io_offset = (next.base_addresses[0] >> 2) << 2;
		  debug!("io offset is 0x{:x}", io_offset)
		  debug!("command is 0x{:x}", shared.command)
		  self.address_port.out_l(Pci::build_address(bus as u8, device as u8, 0, 4));
		  self.data_port.out_w(shared.command | 0x4);
		  debug!("command after bus mastering is 0x{:x}", self.read_header(bus as u8, device as u8).unwrap().shared.command)
		}
		
	      }
	      _ => ()
	    }
	  }
	}
      }
    }
    
    if io_offset != 0 {
      let mut r = Rtl8139::new(io_offset as u16);
      r.init();
      
      r.put_frame("hello, etherworld!".as_bytes());
    }
    
    debug!("not found {}", no_device_count);
    debug!("found {}", device_count);
    (no_device_count, device_count)
  }

}
