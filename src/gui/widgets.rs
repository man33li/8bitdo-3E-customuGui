//! Immediate widgets return edits, never touch hardware. IDs must remain stable
//! and unique within a page. All navigation/widget storage has fixed capacity.
use super::clay::{self, Bounds, Color, Frame, Size, Style};
use std::fmt;
pub const BG:Color=clay::rgb(17,21,29);
pub const PANEL:Color=clay::rgb(28,34,45);
pub const SURFACE:Color=clay::rgb(39,48,63);
pub const TEXT:Color=clay::rgb(241,245,251);
pub const MUTED:Color=clay::rgb(177,190,209);
pub const ACCENT:Color=clay::rgb(86,219,182);
pub const FOCUS:Color=clay::rgb(255,208,102);
pub const BORDER:Color=clay::rgb(78,94,118);
#[derive(Clone,Copy,Default)]
pub struct Input { pub pointer:[f32;2],pub click:bool,pub down:bool,pub activate:bool,pub back:bool,pub left:bool,pub right:bool,pub up:bool,pub next:bool,pub previous:bool,pub chars:[u32;16],pub backspace:bool }
#[derive(Clone,Copy,Default)]
struct Entry { id:u32,bounds:Bounds,adjustable:bool }
pub struct Focus { entries:[Entry;256],count:usize,pub selected:u32,pub dropdown:u32,pub dragging:u32 }
impl Default for Focus {fn default()->Self{Self{entries:[Entry::default();256],count:0,selected:0,dropdown:0,dragging:0}}}
impl Focus {
    pub fn begin(&mut self,input:Input) {
        if self.count>0 {
            let current=self.entries[..self.count].iter().position(|e|e.id==self.selected).unwrap_or(0);
            let adjustable=self.entries[current].adjustable;
            let direction=if input.previous||input.up{-1}else if input.next{1}else{0};
            let horizontal=if !adjustable&&input.left{-1}else if !adjustable&&input.right{1}else{0};
            if direction!=0 {self.selected=self.entries[(current as isize+direction).rem_euclid(self.count as isize) as usize].id;}
            else if horizontal!=0 {
                let source=self.entries[current].bounds;
                let (sx,sy)=(source.x+source.width/2.0,source.y+source.height/2.0);
                let candidate=self.entries[..self.count].iter().filter_map(|e|{let dx=e.bounds.x+e.bounds.width/2.0-sx;let dy=e.bounds.y+e.bounds.height/2.0-sy;if dx*horizontal as f32>1.0{Some((dx.abs()+dy.abs()*3.0,e.id))}else{None}}).min_by(|a,b|a.0.total_cmp(&b.0));
                if let Some((_,id))=candidate{self.selected=id;}
            }
        }
        if input.back{self.dropdown=0;}
        if !input.down{self.dragging=0;}
        self.count=0;
    }
    pub fn reset(&mut self){self.selected=0;self.dropdown=0;self.dragging=0;self.count=0;}
}
pub struct Ui<'a,'b>{pub frame:&'a mut Frame<'b>,pub focus:&'a mut Focus,pub input:Input,pub enabled:bool}
impl Ui<'_,'_>{
    pub fn element(&mut self,name:&'static str,index:u32,style:Style,children:impl FnOnce(&mut Ui<'_,'_>)) {
        let focus=&mut *self.focus;let input=self.input;let enabled=self.enabled;
        self.frame.element(name,index,style,|frame|children(&mut Ui{frame,focus,input,enabled}));
    }
    pub fn scroll(&mut self,name:&'static str,style:Style,children:impl FnOnce(&mut Ui<'_,'_>)) {
        let focus=&mut *self.focus;let input=self.input;let enabled=self.enabled;
        self.frame.scroll(name,style,|frame|children(&mut Ui{frame,focus,input,enabled}));
    }
    pub fn text(&mut self,text:impl fmt::Display,size:u16,color:Color){self.frame.text(text,size,color)}
    pub fn nav_sidebar(&mut self,pages:&[&str],selected:usize,demo:bool,motion:bool)->Option<usize>{
        let mut result=None;
        self.element("sidebar",0,Style::column(Size::Fixed(210.0),Size::Grow).padding(18).gap(12).background(PANEL),|ui|{
            ui.text("ULTIMATE",23,ACCENT);ui.text("CONTROL",18,TEXT);
            ui.text(if demo{"DEMO / NO DEVICE I/O"}else{"Native controller studio"},12,MUTED);
            // 14 pages exceed the 640 px minimum window height: scroll the list, keep the header.
            ui.scroll("nav-scroll",Style::column(Size::Grow,Size::Grow).gap(12),|ui|{
                for(i,label)in pages.iter().enumerate(){if *label=="Motion"&&!motion{continue;}if ui.button("nav",i as u32,label,selected==i){result=Some(i);}}
            });
            ui.text("Tab / arrows: focus",12,MUTED);ui.text("Enter / A: activate",12,MUTED);ui.text("Left / right: adjust",12,MUTED);
        });result
    }
    pub fn controller_diagram(&mut self,texture:&clay::Texture,hotspots:&[(&str,f32,f32)])->Option<usize>{
        let mut result=None;
        self.element("diagram-area",0,Style::column(Size::Grow,Size::Fit).padding(12).gap(12).centered().rounded(12.0).background(PANEL),|ui|{
            let [width,height]=texture.fit(480.0,260.0);
            ui.frame.image("controller-image",Style::row(Size::Fixed(width),Size::Fixed(height)),texture);
            // Labels stay below the illustration, never obscure physical inputs.
            ui.element("diagram-links",0,Style::row(Size::Grow,Size::Fit).gap(8),|ui|{
                for(i,&(label,_,_))in hotspots.iter().enumerate(){if ui.button("hotspot-button",i as u32,label,false){result=Some(i);}}
            });
        });result
    }
    fn interaction(&mut self,name:&'static str,index:u32,adjustable:bool)->(bool,bool,bool){
        let id=clay::id(name,index).id;
        if self.enabled&&self.focus.count<256 {
            self.focus.entries[self.focus.count]=Entry{id,bounds:self.frame.bounds(name,index).unwrap_or_default(),adjustable};self.focus.count+=1;
            if self.focus.selected==0{self.focus.selected=id;}
        }
        let hover=self.enabled&&self.frame.hovered(name,index);
        if hover&&self.input.click{self.focus.selected=id;}
        let focused=self.enabled&&self.focus.selected==id;
        if focused&&(self.input.up||self.input.next||self.input.previous){self.frame.reveal_in_scroll("page-scroll",name,index);self.frame.reveal_in_scroll("nav-scroll",name,index);}
        (hover,focused,self.enabled&&((hover&&self.input.click)||(focused&&self.input.activate)))
    }
    pub fn button(&mut self,name:&'static str,index:u32,label:impl fmt::Display,selected:bool)->bool{
        let(hover,focused,activate)=self.interaction(name,index,false);
        let background=if selected{clay::rgb(28,81,75)}else if hover{clay::rgb(56,68,88)}else{SURFACE};
        let style=Style::row(Size::Grow,Size::Fixed(40.0)).padding(10).rounded(7.0).background(background).border(if focused{FOCUS}else{BORDER},if focused{2}else{1}).centered();
        let color=if self.enabled{TEXT}else{MUTED};
        self.element(name,index,style,|ui|ui.text(label,16,color));activate
    }
    pub fn toggle(&mut self,name:&'static str,index:u32,label:&str,value:bool)->Option<bool>{
        let mut changed=false;
        self.element("toggle-row",clay::id(name,index).id,Style::row(Size::Grow,Size::Fit).gap(14),|ui|{
            ui.element("toggle-label",clay::id(name,index).id,Style::column(Size::Grow,Size::Fixed(40.0)).centered(),|ui|ui.text(label,16,TEXT));
            ui.element("toggle-switch",clay::id(name,index).id,Style::row(Size::Fixed(92.0),Size::Fit),|ui|{changed=ui.button(name,index,if value{"ON"}else{"OFF"},value);});
        }); changed.then_some(!value)
    }
    pub fn range(&mut self,name:&'static str,index:u32,label:&str,value:u8,min:u8,max:u8,percent:bool,offset:u16)->Option<u8>{
        let(hover,focused,_)=self.interaction(name,index,true);let id=clay::id(name,index).id;
        if hover&&self.input.click{self.focus.dragging=id;}
        let mut next=value;
        if focused{if self.input.left{next=next.saturating_sub(1).max(min);}if self.input.right{next=next.saturating_add(1).min(max);}}
        if self.enabled&&self.focus.dragging==id&&self.input.down{if let Some(b)=self.frame.bounds(name,index){next=(f32::from(min)+((self.input.pointer[0]-b.x-14.0)/(b.width-28.0)).clamp(0.0,1.0)*f32::from(max-min)).round() as u8;}}
        let display=if percent{u16::from(value)*100/u16::from(max)}else{u16::from(value)+offset};
        self.element(name,index,Style::column(Size::Grow,Size::Fixed(65.0)).padding(10).gap(8).rounded(7.0).background(SURFACE).border(if focused{FOCUS}else{BORDER},if focused{2}else{1}),|ui|{
            ui.text(format_args!("{label}   {display}{}",if percent{"%"}else{""}),16,if ui.enabled{TEXT}else{MUTED});
            ui.element("slider-track",id,Style::row(Size::Grow,Size::Fixed(7.0)).background(BORDER).rounded(3.0),|ui|{
                ui.element("slider-fill",id,Style::row(Size::Percent(((f32::from(value)-f32::from(min))/f32::from(max-min)).clamp(0.001,1.0)),Size::Fixed(7.0)).background(ACCENT).rounded(3.0),|_|{});
            });
        });(next!=value).then_some(next)
    }
    pub fn text_input(&mut self,name:&'static str,value:&mut String){
        self.text_input_limit(name,value,16);
    }
    pub fn text_input_limit(&mut self,name:&'static str,value:&mut String,max:usize){
        let(_,focused,_)=self.interaction(name,0,true);
        if focused&&self.enabled{
            if self.input.backspace{value.pop();}
            for code in self.input.chars{if let Some(c)=char::from_u32(code){if !c.is_control()&&value.encode_utf16().count()+c.len_utf16()<=max{value.push(c);}}}
        }
        self.element(name,0,Style::row(Size::Grow,Size::Fixed(42.0)).padding(10).background(SURFACE).border(if focused{FOCUS}else{BORDER},1),|ui|ui.text(format_args!("{value}{}",if focused{" |"}else{""}),16,TEXT));
    }
    pub fn plot(&mut self,name:&'static str,index:u32,x:f32,y:f32){
        self.plot_image(name,index,x,y,None);
    }
    pub fn plot_image(&mut self,name:&'static str,index:u32,x:f32,y:f32,marker:Option<&clay::Texture>){
        let id=clay::id(name,index);
        self.element(name,index,Style::row(Size::Fixed(130.0),Size::Fixed(130.0)).border(BORDER,1).background(PANEL),|ui|{
            ui.element("plot-x",id.id,Style::row(Size::Fixed(130.0),Size::Fixed(1.0)).background(BORDER).floating(Some(id),0.0,65.0,1),|_|{});
            ui.element("plot-y",id.id,Style::row(Size::Fixed(1.0),Size::Fixed(130.0)).background(BORDER).floating(Some(id),65.0,0.0,1),|_|{});
            if let Some(marker)=marker{
                ui.frame.image_index("plot-art-dot",id.id,Style::row(Size::Fixed(20.0),Size::Fixed(20.0)).floating(Some(id),55.0+x.clamp(-1.0,1.0)*55.0,55.0-y.clamp(-1.0,1.0)*55.0,2),marker);
            }else{
                ui.element("plot-dot",id.id,Style::row(Size::Fixed(8.0),Size::Fixed(8.0)).rounded(4.0).background(ACCENT).floating(Some(id),61.0+x.clamp(-1.0,1.0)*60.0,61.0-y.clamp(-1.0,1.0)*60.0,2),|_|{});
            }
        });
    }
    pub fn curve(&mut self,curve:&crate::profile::StickCurve){
        let id=clay::id("curve-graph",0);
        self.element("curve-graph",0,Style::row(Size::Fixed(276.0),Size::Fixed(120.0)).background(PANEL).border(BORDER,1),|ui|{
            for (segment,pair) in curve.points.windows(2).enumerate(){
                for step in 0..24{let t=step as f32/23.0;let x=f32::from(pair[0].input.get())*(1.0-t)+f32::from(pair[1].input.get())*t;let y=f32::from(pair[0].output.get())*(1.0-t)+f32::from(pair[1].output.get())*t;
                    ui.element("curve-dot",(segment*24+step) as u32,Style::row(Size::Fixed(3.0),Size::Fixed(3.0)).background(ACCENT).floating(Some(id),x*2.76,120.0-y*1.2,1),|_|{});
                }
            }
        });
    }
    pub fn dropdown(&mut self,name:&'static str,index:u32,options:&[&str],value:usize)->Option<usize>{
        let id=clay::id(name,index);let mut result=None;
        if self.button(name,index,format_args!("{}  v",options.get(value).unwrap_or(&"Unavailable")),false){self.focus.dropdown=if self.focus.dropdown==id.id{0}else{id.id};}
        if self.focus.dropdown==id.id {
            let view=self.frame.bounds("page-scroll",0).unwrap_or(Bounds{height:600.0,..Bounds::default()});
            let rows=(((view.height-20.0)/42.0)as usize).clamp(1,10);
            let height=options.len().min(rows)as f32*42.0+12.0;
            let offset=self.frame.bounds(name,index).map_or(42.0,|b|{
                let below=b.y+42.0;
                let top=if below+height>view.y+view.height{b.y-height-4.0}else{below};
                top.clamp(view.y+4.0,(view.y+view.height-height-4.0).max(view.y+4.0))-b.y
            });
            let width=if options.len()>10{options.len().div_ceil(rows)as f32*154.0+12.0}else{420.0};
            self.element("dropdown-popup",id.id,Style::row(Size::Fixed(width),Size::Fit).padding(6).gap(4).background(PANEL).border(FOCUS,1).rounded(8.0).floating(Some(id),0.0,offset,30),|ui|{
                for (column,chunk) in options.chunks(rows).enumerate(){
                    ui.element("dropdown-column",id.id.wrapping_add(column as u32),Style::column(Size::Grow,Size::Fit).gap(2),|ui|{
                        for (i,option) in chunk.iter().enumerate(){let i=column*rows+i;if ui.button("dropdown-option",id.id.wrapping_add(i as u32),option,i==value){result=Some(i);ui.focus.dropdown=0;}}
                    });
                }
            });
        } result
    }
    pub fn segmented(&mut self,name:&'static str,options:&[&str],selected:usize)->Option<usize>{let mut result=None;self.element(name,0,Style::row(Size::Grow,Size::Fit).gap(8),|ui|{for(i,label)in options.iter().enumerate(){if ui.button(name,i as u32+1,label,i==selected){result=Some(i)}}});result}
    pub fn list_row(&mut self,index:u32,label:impl fmt::Display,detail:impl fmt::Display,selected:bool)->bool{self.button("list-row",index,format_args!("{label}     {detail}"),selected)}
    pub fn page_header(&mut self,title:&str,subtitle:&str){self.element("page-header",0,Style::column(Size::Grow,Size::Fit).gap(8),|ui|{ui.text(title,30,TEXT);ui.text(subtitle,15,MUTED);});}
    pub fn progress(&mut self,name:&'static str,value:Option<f32>,label:&str){self.text(label,15,MUTED);self.element(name,0,Style::row(Size::Grow,Size::Fixed(10.0)).background(BORDER).rounded(5.0),|ui|{ui.element(name,1,Style::row(Size::Percent(value.unwrap_or(0.15).clamp(0.001,1.0)),Size::Fixed(10.0)).background(ACCENT).rounded(5.0),|_|{});});}
    pub fn toast(&mut self,message:&str)->bool{let mut dismiss=false;self.element("toast",0,Style::row(Size::Grow,Size::Fit).padding(12).gap(12).background(clay::rgb(28,65,63)).border(ACCENT,1).rounded(8.0),|ui|{ui.element("toast-text",0,Style::column(Size::Grow,Size::Fit),|ui|ui.text(message,15,TEXT));ui.element("toast-close",0,Style::row(Size::Fixed(90.0),Size::Fit),|ui|{dismiss=ui.button("dismiss",0,"Dismiss",false)});});dismiss}
    /// Optional backup choice performs backup before staging; never device I/O.
    pub fn modal(&mut self,title:&str,message:&str,backup:bool)->Option<ModalChoice>{let mut result=None;self.element("modal-shade",0,Style::column(Size::Grow,Size::Grow).centered().background(Color{r:0.0,g:0.0,b:0.0,a:190.0}).floating(None,0.0,0.0,100),|ui|{ui.element("modal-card",0,Style::column(Size::Fixed(620.0),Size::Fit).padding(26).gap(20).rounded(12.0).background(PANEL).border(FOCUS,2),|ui|{ui.text(title,25,TEXT);ui.text(message,17,MUTED);ui.element("modal-actions",0,Style::row(Size::Grow,Size::Fit).gap(12),|ui|{if ui.button("modal-cancel",0,"Cancel",false){result=Some(ModalChoice::Cancel)}if backup&&ui.button("modal-backup",0,"Save backup first",false){result=Some(ModalChoice::Backup)}if ui.button("modal-confirm",0,"Confirm",true){result=Some(ModalChoice::Confirm)}});});});if self.input.back{result=Some(ModalChoice::Cancel)}result}
}

pub enum ModalChoice { Cancel, Confirm, Backup }
