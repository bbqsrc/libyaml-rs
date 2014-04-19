#![crate_type = "bin"]
#![feature(globs)]

extern crate libc;

use std::fmt;
use std::cast;
use std::str;
use std::io::fs;

use std::strbuf::StrBuf;
use std::io::MemWriter;

use ll = yamlll;

mod yamlll;

pub struct Parser {
    _c: ll::yaml_parser_t
}

pub struct Document {
    _c: ll::yaml_document_t
}

pub struct Node {
    _c: ll::yaml_node_t,
    document: Document,
    index: uint
}

pub struct Emitter {
    _c: ll::yaml_emitter_t
}

#[deriving(Clone, Show)]
enum NodeOption<T, U, V> {
    NoNode,
    ScalarNode(T),
    SequenceNode(U),
    MappingNode(V)
}

impl Parser {
    fn new() -> Parser {
        Parser {
            _c: unsafe {
                let x = std::mem::uninit();
                ll::yaml_parser_initialize(x);
                *x
            }
        }
    }

    fn set_input_file(&mut self, filename: ~str) -> Result<(), &'static str> {
        unsafe {
            let file: *mut libc::FILE = cast::transmute(libc::fopen(
                    filename.to_c_str().unwrap(), "rb".to_c_str().unwrap()));

            if file.is_null() {
                return Err("file is null");
            }
            
            ll::yaml_parser_set_input_file(
                &mut self._c, file
            );
            
            Ok(())
        }
    }

    fn set_input_string(&mut self, input: ~str) {
        let c_str = input.to_c_str();
        let c_str_len = input.len();
        unsafe {
            ll::yaml_parser_set_input_string(&mut self._c, c_str.unwrap() as *u8, c_str_len as u64)
        }
    }

    fn parse(&mut self) -> Result<(), &'static str> { // should return Document
        unsafe {
            let mut event: ll::yaml_event_t = std::mem::uninit();
            let mut done = false;

            while !done {
                let res = match ll::yaml_parser_parse(&mut self._c, &mut event) {
                    1 => Ok(()),
                    _ => Err("parsing failed")
                };

                match res {
                    Ok(_) => (),
                    Err(e) => return res
                };
                
                if event._type == ll::YAML_STREAM_END_EVENT {
                    done = true;
                }
                
                println!("{} {}", res, done);

                if done {
                    return res;
                }
            }
            
            ll::yaml_event_delete(&mut event);

            Ok(())
        }
    }

    fn load(&mut self, document: &mut Document) -> Result<(), &'static str> {
        unsafe {
            match ll::yaml_parser_load(&mut self._c, &mut document._c) {
                1 => Ok(()),
                _ => Err("loading failed")
            }
        }
    }
}

impl Node {
    fn data(&self) -> NodeOption<&str, ~[~str], Node> {
        match self._c._type {
            ll::YAML_NO_NODE => NoNode,
            ll::YAML_SCALAR_NODE => {
                unsafe {
                    let mut_self: &mut Node = cast::transmute(self);
                    let node_data = mut_self._c.data.scalar();

                    let cv = cast::transmute(std::raw::Slice::<u8> {
                        data: (*node_data).value as *u8,
                        len: (*node_data).length as uint
                    });
                    //let v = cv.as_slice();
                    
                    ScalarNode(str::from_utf8(cv).unwrap())
                }
            },
            ll::YAML_SEQUENCE_NODE => SequenceNode(~[~"test"]),
            ll::YAML_MAPPING_NODE => MappingNode(*self)
        }
    }
}

impl Index<~str, Option<Node>> for Node {
    fn index(&self, key: &~str) -> Option<Node> {
        let doc = self.document;

        match self._c._type {
            ll::YAML_NO_NODE => fail!("this shouldn't even be possible."),
            ll::YAML_SCALAR_NODE => fail!("scalar nodes have no indices"),
            ll::YAML_SEQUENCE_NODE => fail!("sequence nodes do not accept string indices"),
            ll::YAML_MAPPING_NODE => {
                unsafe {
                    let mut_self: &mut Node = cast::transmute(self);
                    let node_data = mut_self._c.data.mapping();
                    
                    let pairs = (*node_data).pairs;
    
                    let mut pair = pairs.start;
   
                    while pair != pairs.end && (*pair).value != 0 {
                        
                        // Find a scalar or skip
                        let mut node = doc.get_node((*pair).key as uint).unwrap();
                        
                        match node._c._type {
                            ll::YAML_SCALAR_NODE => {
                                let node_data = node._c.data.scalar();
                                
                                let cv = std::c_vec::CVec::new((*node_data).value, (*node_data).length as uint);
                                let v = cv.as_slice();
                        
                                let text = match str::from_utf8(v) {
                                    Some(t) => {
                                        if (t == *key) {
                                            return Some(doc.get_node((*pair).value as uint).unwrap());
                                        }
                                    },
                                    None => fail!("NOPE.")
                                };
                            },
                            _ => ()
                        }
                        
                        pair = pair.offset(1);
                    }
                }
                None
            }
        }

    }
}

impl fmt::Show for Node {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        unsafe {
            let mut_self: &mut Node = cast::transmute(self);
            
            write!(f.buf, "{}", match self._c._type {
                ll::YAML_NO_NODE => ~"None",
                ll::YAML_SCALAR_NODE => {
                    let node_data = mut_self._c.data.scalar();
                    
                    let cv = std::c_vec::CVec::new((*node_data).value, (*node_data).length as uint);
                    let v = cv.as_slice();
            
                    let text = match str::from_utf8(v) {
                        Some(t) => format!("{}", t),
                        None => ~"<None>"
                    };
                    
                    let style = match (*node_data).style {
                        ll::YAML_ANY_SCALAR_STYLE => "any",
                        ll::YAML_PLAIN_SCALAR_STYLE => "plain",
                        ll::YAML_SINGLE_QUOTED_SCALAR_STYLE => "single quoted",
                        ll::YAML_DOUBLE_QUOTED_SCALAR_STYLE => "double quoted",
                        ll::YAML_LITERAL_SCALAR_STYLE => "literal",
                        ll::YAML_FOLDED_SCALAR_STYLE => "folded"
                    };

                    format!("Scalar ( {}, style: {}, length: {} )", text, style, (*node_data).length)
                }
                ll::YAML_SEQUENCE_NODE => {
                    let node_data = mut_self._c.data.sequence();
                    let items = (*node_data).items;

                    let mut buf = StrBuf::new();
                    buf.push_str("List [ ");

                    let mut item = items.start;

                    while item != items.end && *item != 0 {
                        buf.push_str(format!("{}, ", *item));
                        item = item.offset(1);
                    }
                  
                    let buf_len = buf.len();
                    buf.truncate(buf_len-2);

                    buf.push_str(" ]");

                    buf.into_owned()
                },
                ll::YAML_MAPPING_NODE => {
                    let node_data = mut_self._c.data.mapping();
                    let pairs = (*node_data).pairs;

                    let mut buf = StrBuf::new();
                    buf.push_str("Map { ");
                    let mut pair = pairs.start;

                    while pair != pairs.end && (*pair).value != 0 {
                        buf.push_str(format!("{}: {}, ", (*pair).key, (*pair).value));
                        pair = pair.offset(1);
                    }
                  
                    let buf_len = buf.len();
                    buf.truncate(buf_len-2);

                    buf.push_str(" }");

                    buf.into_owned()
                }
            })
            /*
            write!(f.buf, "({}, {}, {})", match self._c._type {
                ll::YAML_NO_NODE => "None",
                ll::YAML_SCALAR_NODE => "Scalar",
                ll::YAML_SEQUENCE_NODE => "Sequence",
                ll::YAML_MAPPING_NODE => "Mapping",
            }, (*node_data).length, text)*/
        }
    }
}

impl Document {
    fn new() -> Document {
        unsafe {
            Document {
                _c: std::mem::uninit()
            }
        }
    }

    fn get_root_node(&mut self) -> Result<Node, &'static str> {
        unsafe {
            let x = ll::yaml_document_get_root_node(&mut self._c);
            if !x.is_null() { 
                Ok(Node { _c: *x, document: *self, index: 1 })
            } else { Err("root node is null pointer") }
        }
    }

    fn get_node(&self, index: uint) -> Option<Node> {
        unsafe {
            let x = ll::yaml_document_get_node(&self._c, index as libc::c_int);
            if !x.is_null() {
                Some(Node { _c: *x, document: *self, index: index })
            } else {
                None
            }
        }
    }
}


extern fn dumps_callback(ext: *mut libc::c_void, buffer: *mut u8, size: u64) -> i32 {
    unsafe {
        let w: *mut MemWriter = cast::transmute(ext);

        let cv = std::c_vec::CVec::new(buffer, size as uint);
        let v = cv.as_slice();

        (*w).write(v);
        //println!("{}", str::from_utf8(v).unwrap());
    }
    
    return 0;
}

impl Emitter {
    fn new() -> Emitter {
        Emitter {
            _c: unsafe {
                let x = std::mem::uninit();
                ll::yaml_emitter_initialize(x);
                *x
            }
        }
    }
   
    fn set_output_file(&mut self, filename: ~str) {
        unsafe {
            let file = cast::transmute(
                libc::fopen(filename.to_c_str().unwrap(), "wb".to_c_str().unwrap())
            );
            ll::yaml_emitter_set_output_file(
                &mut self._c, file
            );
        }
    }

    fn dump(&mut self, document: &mut Document) -> Result<(), &'static str> {
        unsafe {
            match ll::yaml_emitter_dump(&mut self._c, &mut document._c) {
                1 => Ok(()),
                _ => Err("dumping failed")
            }
        }
    }

    fn dumps(&mut self, document: &mut Document) -> &str {
        unsafe {
            let mut w: *mut MemWriter = &mut MemWriter::new();

            ll::yaml_emitter_set_output(
                &mut self._c,
                cast::transmute(dumps_callback),
                cast::transmute_mut_lifetime(cast::transmute(w))
            );

            self.dump(document);

            str::from_utf8((*w).get_ref()).unwrap()
        }
    }

    fn flush(&mut self) {
        unsafe {
            ll::yaml_emitter_flush(&mut self._c);
        }
    }
}

impl Drop for Emitter {
    fn drop(&mut self) {
        unsafe {
            ll::yaml_emitter_delete(&mut self._c);
        }
    }
}

pub fn get_version_string() -> ~str {
    unsafe {
        str::raw::from_c_str(ll::yaml_get_version_string())
    }
}

pub fn load(file: &mut fs::File) -> Document {
    let data = match file.read_to_str() {
        Ok(v) => v,
        Err(e) => fail!(e)
    };

    loads(data)
}

pub fn loads(data: ~str) -> Document {
    let mut parser = Parser::new();
    let mut document = Document::new();

    parser.set_input_string(data);
    parser.load(&mut document);

    document
}

fn main() {
    println!("{}", get_version_string());

    /*
    let mut parser = Parser::new();
    let mut emitter = Emitter::new();
    
    match parser.set_input_file(~"test.yaml") {
        Ok(_) => (),
        Err(e) => fail!(e)
    }

    let mut document = Document::new();
            
    match parser.load(&mut document) {
        Ok(_) => (),
        Err(e) => fail!(e)
    };
    */

    let mut file = fs::File::open(&Path::new("test.yaml")).unwrap();

    let mut document = load(&mut file);

    match document.get_root_node() {
        Ok(node) => println!("Root: {}\n", node),
        Err(e) => fail!(e)
    };

    let mut i = 1;
    loop {
        match document.get_node(i) {
            Some(node) => println!("{}: {}", i, node),
            None => break//println!("no node :(")
        }

        i += 1;
    }
    println!("");

    let root = document.get_root_node().unwrap();

    let dunno = root[~"Config"].unwrap()[~"hfst"].unwrap()[~"Gen"].unwrap().data();

    println!("{}", dunno);
    //println!("{}", emitter.dumps(&mut document));
}

