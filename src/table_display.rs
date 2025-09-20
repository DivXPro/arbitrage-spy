use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
    Frame, Terminal,
};
use tui_logger::{TuiLoggerWidget, TuiLoggerLevelOutput};
use std::io::{self, Stdout};
use std::time::Duration;
use tokio::sync::mpsc;
use log::{info};
use chrono;
use crate::price_calculator::PriceCalculator;
use crate::pair_manager::PairData;

#[derive(Clone, Debug)]
pub struct PairDisplay {
    pub rank: usize,
    pub pair: String,
    pub dex: String,
    pub price: String,
    pub liquidity: String,
    pub last_update: String,
}

#[derive(Debug, Clone)]
pub enum DisplayMessage {
    /// 全量更新 - 替换所有数据
    FullUpdate(Vec<PairDisplay>),
    /// 局部更新 - 更新指定索引的数据
    PartialUpdate { index: usize, data: PairDisplay },
    /// 批量局部更新 - 更新多个指定索引的数据
    BatchPartialUpdate(Vec<(usize, PairDisplay)>),
    /// 关闭显示
    Shutdown,
}

/// PairData转换工具
pub struct PairDisplayConverter;

impl PairDisplayConverter {
    /// 将单个PairData转换为PairDisplay
    pub fn convert_single(pair: &PairData, rank: usize) -> PairDisplay {
        let price = match PriceCalculator::calculate_price_from_pair(pair) {
            Ok(price_value) => PriceCalculator::format_price(&price_value),
            Err(_) => "$0.000000".to_string(),
        };
        
        PairDisplay {
            rank,
            pair: format!("{}/{}", pair.token0.symbol, pair.token1.symbol),
            dex: pair.dex.clone(),
            price,
            liquidity: format!("${:.0}", pair.reserve_usd.parse::<f64>().unwrap_or(0.0)),
            last_update: chrono::Utc::now().format("%H:%M:%S").to_string(),
        }
    }
    
    /// 将PairData列表转换为PairDisplay列表
    pub fn convert_list(pairs: &[PairData]) -> Result<Vec<PairDisplay>> {
        let display_pairs: Vec<PairDisplay> = pairs
            .iter()
            .enumerate()
            .map(|(index, pair)| Self::convert_single(pair, index + 1))
            .collect();
        
        Ok(display_pairs)
    }
    
    /// 将PairData向量转换为PairDisplay向量（消费输入）
    pub fn convert_owned(pairs: Vec<PairData>) -> Result<Vec<PairDisplay>> {
        let display_pairs: Vec<PairDisplay> = pairs
            .into_iter()
            .enumerate()
            .map(|(index, pair)| Self::convert_single(&pair, index + 1))
            .collect();
        
        Ok(display_pairs)
    }
    
    /// 为事件处理创建PairDisplay（使用自定义错误处理）
    pub fn convert_for_event(pair: &PairData, rank: usize) -> PairDisplay {
        let price = match PriceCalculator::calculate_price_from_pair(pair) {
            Ok(price_value) => PriceCalculator::format_price(&price_value),
            Err(_) => "$0.000000".to_string(),
        };
        
        PairDisplay {
            rank,
            pair: format!("{}/{}", pair.token0.symbol, pair.token1.symbol),
            dex: pair.dex.clone(),
            price,
            liquidity: format!("${:.0}", pair.reserve_usd.parse::<f64>().unwrap_or(0.0)),
            last_update: chrono::Utc::now().format("%H:%M:%S").to_string(),
        }
    }
}

pub struct TableDisplay {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    receiver: mpsc::Receiver<DisplayMessage>,
    show_logs: bool,
    tui_logger_state: tui_logger::TuiWidgetState,
    initial_data: Vec<PairDisplay>,
    scroll_offset: usize,
    visible_rows: usize,
    all_pairs: Vec<PairDisplay>,
}

impl TableDisplay {
    pub fn new(receiver: mpsc::Receiver<DisplayMessage>, initial_data: Vec<PairDisplay>) -> Result<Self> {
        let backend = CrosstermBackend::new(io::stdout());
        let terminal = Terminal::new(backend)?;
        
        info!("📊 接收到 {} 个初始交易对数据", initial_data.len());
        
        let all_pairs = initial_data.clone();
        
        Ok(Self {
            terminal,
            receiver,
            show_logs: true,
            tui_logger_state: tui_logger::TuiWidgetState::new(),
            initial_data,
            scroll_offset: 0,
            visible_rows: 10,
            all_pairs,
        })
    }
    

    
    pub async fn start_display(&mut self) -> Result<()> {
        // 启用原始模式并进入备用屏幕
        terminal::enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen)?;
        
        // 使用初始数据作为当前显示的数据
        let mut current_pairs = self.initial_data.clone();
        
        // 初始渲染
        let visible_pairs = self.get_visible_pairs(&current_pairs);
        self.terminal.draw(|f| {
            if self.show_logs {
                Self::render_ui_with_logs(f, &current_pairs, &mut self.tui_logger_state);
            } else {
                Self::render_ui_static(f, &visible_pairs, self.scroll_offset, current_pairs.len(), self.visible_rows);
            }
        })?;
        
        info!("🚀 TableDisplay 已启动，显示 {} 个初始交易对", current_pairs.len());
        println!("🚀 TableDisplay 已启动，显示 {} 个初始交易对", current_pairs.len());
        
        loop {
            tokio::select! {
                message = self.receiver.recv() => {
                    match message {
                        Some(DisplayMessage::FullUpdate(pairs)) => {
                            current_pairs = pairs;
                            let visible_pairs = self.get_visible_pairs(&current_pairs);
                            let _ = self.terminal.draw(|f| {
                                if self.show_logs {
                                     Self::render_ui_with_logs(f, &current_pairs, &mut self.tui_logger_state);
                                 } else {
                                     Self::render_ui_static(f, &visible_pairs, self.scroll_offset, current_pairs.len(), self.visible_rows);
                                 }
                            });
                        }
                        Some(DisplayMessage::PartialUpdate { index, data }) => {
                            if index < current_pairs.len() {
                                current_pairs[index] = data;
                                let visible_pairs = self.get_visible_pairs(&current_pairs);
                                let _ = self.terminal.draw(|f| {
                                    if self.show_logs {
                                         Self::render_ui_with_logs(f, &current_pairs, &mut self.tui_logger_state);
                                     } else {
                                         Self::render_ui_static(f, &visible_pairs, self.scroll_offset, current_pairs.len(), self.visible_rows);
                                     }
                                });
                            }
                        }
                        Some(DisplayMessage::BatchPartialUpdate(updates)) => {
                            for (index, data) in updates {
                                if index < current_pairs.len() {
                                    current_pairs[index] = data;
                                }
                            }
                            let visible_pairs = self.get_visible_pairs(&current_pairs);
                            let _ = self.terminal.draw(|f| {
                                if self.show_logs {
                                     Self::render_ui_with_logs(f, &current_pairs, &mut self.tui_logger_state);
                                 } else {
                                     Self::render_ui_static(f, &visible_pairs, self.scroll_offset, current_pairs.len(), self.visible_rows);
                                 }
                            });
                        }
                        Some(DisplayMessage::Shutdown) => break,
                        None => break,
                    }
                }
                _ = tokio::time::sleep(Duration::from_millis(100)) => {
                    if event::poll(Duration::from_millis(0))? {
                        if let Event::Key(key) = event::read()? {
                            match key.code {
                                KeyCode::Char('c') if key.modifiers.contains(event::KeyModifiers::CONTROL) => {
                                    break;
                                }
                                KeyCode::Char('l') => {
                                    self.show_logs = !self.show_logs;
                                    let visible_pairs = self.get_visible_pairs(&current_pairs);
                                    let scroll_offset = self.scroll_offset;
                                    let visible_rows = self.visible_rows;
                                    let _ = self.terminal.draw(|f| {
                                        if self.show_logs {
                                             Self::render_ui_with_logs(f, &current_pairs, &mut self.tui_logger_state);
                                         } else {
                                             Self::render_ui_static(f, &visible_pairs, scroll_offset, current_pairs.len(), visible_rows);
                                         }
                                    });
                                }
                                KeyCode::Up => {
                                    if !self.show_logs {
                                        // 只在表格模式下，向上滚动
                                        if self.scroll_offset > 0 {
                                            self.scroll_offset -= 1;
                                            let visible_pairs = self.get_visible_pairs(&current_pairs);
                                            let _ = self.terminal.draw(|f| {
                                                Self::render_ui_static(f, &visible_pairs, self.scroll_offset, current_pairs.len(), self.visible_rows);
                                            });
                                        }
                                    }
                                    // 在日志模式下，忽略方向键，不进行滚动
                                }
                                KeyCode::Down => {
                                    if !self.show_logs {
                                        // 只在表格模式下，向下滚动
                                        let max_offset = if current_pairs.len() > self.visible_rows {
                                            current_pairs.len() - self.visible_rows
                                        } else {
                                            0
                                        };
                                        if self.scroll_offset < max_offset {
                                            self.scroll_offset += 1;
                                            let visible_pairs = self.get_visible_pairs(&current_pairs);
                                            let _ = self.terminal.draw(|f| {
                                                Self::render_ui_static(f, &visible_pairs, self.scroll_offset, current_pairs.len(), self.visible_rows);
                                            });
                                        }
                                    }
                                    // 在日志模式下，忽略方向键，不进行滚动
                                }
                                _ => {
                                    // 在日志模式下，忽略其他可能导致滚动的键盘事件
                                    // 只保留基本的显示功能，不处理滚动相关的键盘事件
                                }
                            }
                        }
                    }
                }
            }
        }
        
        // 恢复终端状态
        terminal::disable_raw_mode()?;
        execute!(io::stdout(), LeaveAlternateScreen)?;
        println!("表格显示已停止");
        
        Ok(())
    }

    fn get_visible_pairs(&self, pairs: &[PairDisplay]) -> Vec<PairDisplay> {
        let start = self.scroll_offset;
        let end = std::cmp::min(start + self.visible_rows, pairs.len());
        pairs[start..end].to_vec()
    }

    fn map_key_to_tui_event(&self, key_code: KeyCode) -> Option<tui_logger::TuiWidgetEvent> {
        // 禁用所有滚动相关的键位映射，日志区域不再支持滚动
        // 只保留基本的显示控制功能
        use tui_logger::TuiWidgetEvent;
        match key_code {
            KeyCode::Char('h') => Some(TuiWidgetEvent::HideKey),
            KeyCode::Char('f') => Some(TuiWidgetEvent::FocusKey),
            KeyCode::Esc => Some(TuiWidgetEvent::EscapeKey),
            _ => None,
        }
    }

    fn render_ui_with_logs(f: &mut Frame, pairs: &[PairDisplay], tui_logger_state: &mut tui_logger::TuiWidgetState) {
        // Split screen: table on top, logs on bottom with better proportions
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
            .margin(0)
            .split(f.area());
        
        Self::render_table_area(f, chunks[0], pairs, true);
        Self::render_log_area(f, chunks[1], tui_logger_state);
    }

    fn render_log_area(f: &mut Frame, area: Rect, tui_logger_state: &mut tui_logger::TuiWidgetState) {
        let tui_logger_widget = TuiLoggerWidget::default()
            .block(Block::default()
                .title("📋 日志输出 (按 'l' 切换显示)")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)))
            .output_separator(' ')  // 使用空格分隔符，更整齐
            .output_timestamp(Some("%H:%M:%S".to_string()))  // 简化时间戳格式
            .output_level(Some(TuiLoggerLevelOutput::Abbreviated))
            .output_target(false)  // 隐藏target，减少混乱
            .output_file(false)
            .output_line(false)
            .style_error(Style::default().fg(Color::Red))
            .style_warn(Style::default().fg(Color::Yellow))
            .style_info(Style::default().fg(Color::Green))
            .style_debug(Style::default().fg(Color::Blue))
            .style_trace(Style::default().fg(Color::Magenta))
            .state(tui_logger_state);
        
        f.render_widget(tui_logger_widget, area);
    }

    fn render_table_area(f: &mut Frame, area: Rect, pairs: &[PairDisplay], show_logs: bool) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3), // 标题
                Constraint::Min(0),    // 表格
                Constraint::Length(3), // 提示信息
            ])
            .split(area);
        
        // 渲染标题
        let title = Paragraph::new("🚀 实时交易对监控")
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(title, chunks[0]);
        
        // 渲染表格
        if !pairs.is_empty() {
            let header_cells = ["排名", "交易对", "DEX", "交易对价格", "流动性", "最后更新"]
                .iter()
                .map(|h| Cell::from(*h).style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));
            let header = Row::new(header_cells).height(1).bottom_margin(1);
            
            let rows = pairs.iter().map(|pair| {
                let cells = vec![
                    Cell::from(pair.rank.to_string()),
                    Cell::from(pair.pair.clone()),
                    Cell::from(pair.dex.clone()),
                    Cell::from(pair.price.clone()),
                    Cell::from(pair.liquidity.clone()),
                    Cell::from(pair.last_update.clone()),
                ];
                Row::new(cells).height(1).bottom_margin(1)
            });
            
            let table = Table::new(rows, [
                Constraint::Length(4),  // 排名
                Constraint::Length(12), // 交易对
                Constraint::Length(8),  // DEX
                Constraint::Length(12), // 价格
                Constraint::Length(10), // 流动性
                Constraint::Length(12), // 最后更新
            ])
            .header(header)
            .block(Block::default().borders(Borders::ALL).title("交易对数据"))
            .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED))
            .highlight_symbol(">> ");
            
            f.render_widget(table, chunks[1]);
        } else {
            let no_data = Paragraph::new("暂无数据...")
                .style(Style::default().fg(Color::Gray))
                .block(Block::default().borders(Borders::ALL).title("交易对数据"));
            f.render_widget(no_data, chunks[1]);
        }
        
        // 渲染提示信息
        let help_text = if show_logs {
            "按 Ctrl+C 退出 | 按 'l' 隐藏日志 | 方向键/鼠标滚轮滚动日志 | 'h'隐藏/显示级别 | '+'增加级别 | '-'减少级别"
        } else {
            "按 Ctrl+C 退出 | 按 'l' 显示日志"
        };
        
        let help = Paragraph::new(help_text)
            .style(Style::default().fg(Color::Gray))
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(help, chunks[2]);
    }

    fn render_ui_static(f: &mut Frame, pairs: &[PairDisplay], scroll_offset: usize, total_pairs: usize, visible_rows: usize) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3), // 标题
                Constraint::Min(0),    // 表格
                Constraint::Length(3), // 提示信息
            ])
            .split(f.area());
        
        // 渲染标题
        let title = Paragraph::new("🚀 实时交易对监控")
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(title, chunks[0]);
        
        // 渲染表格
        if !pairs.is_empty() {
            let header_cells = ["排名", "交易对", "DEX", "交易对价格", "流动性", "Reserve0", "Reserve1", "最后更新"]
                .iter()
                .map(|h| Cell::from(*h).style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));
            let header = Row::new(header_cells).height(1).bottom_margin(1);
            
            let rows = pairs.iter().map(|pair| {
                let cells = vec![
                    Cell::from(pair.rank.to_string()),
                    Cell::from(pair.pair.clone()),
                    Cell::from(pair.dex.clone()),
                    Cell::from(pair.price.clone()),
                    Cell::from(pair.liquidity.clone()),
                    Cell::from(pair.last_update.clone()),
                ];
                Row::new(cells).height(1)
            });
            
            let table = Table::new(rows, &[
                Constraint::Length(4),  // 排名
                Constraint::Length(12), // 交易对
                Constraint::Length(8),  // DEX
                Constraint::Length(12), // 价格
                Constraint::Length(10), // 流动性
                Constraint::Length(15), // Reserve0
                Constraint::Length(15), // Reserve1
                Constraint::Length(12), // 最后更新
            ])
            .header(header)
            .block(Block::default().borders(Borders::ALL).title("交易对数据"));
            f.render_widget(table, chunks[1]);
        } else {
            let no_data = Paragraph::new("等待数据加载...")
                .style(Style::default().fg(Color::Gray))
                .block(Block::default().borders(Borders::ALL).title("交易对数据"));
            f.render_widget(no_data, chunks[1]);
        }
        
        // 渲染提示信息
        let scroll_info = if total_pairs > visible_rows {
            format!("按 Ctrl+C 退出监控 | 按 ↑↓ 滚动 | 显示 {}-{}/{} | 数据实时更新中...", 
                scroll_offset + 1, 
                std::cmp::min(scroll_offset + visible_rows, total_pairs), 
                total_pairs)
        } else {
            format!("按 Ctrl+C 退出监控 | 显示 {}/{} | 数据实时更新中...", total_pairs, total_pairs)
        };
        let help = Paragraph::new(scroll_info)
            .style(Style::default().fg(Color::Yellow))
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(help, chunks[2]);
    }
}