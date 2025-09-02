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
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, Widget},
    Frame, Terminal,
};
use tui_logger::{TuiLoggerWidget, TuiLoggerLevelOutput};
use std::io::{self, Stdout};
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Clone, Debug)]
pub struct PairDisplay {
    pub rank: usize,
    pub pair: String,
    pub dex: String,
    pub price: String,
    pub change_24h: String,
    pub liquidity: String,
    pub last_update: String,
}

#[derive(Debug, Clone)]
pub enum DisplayMessage {
    UpdateData(Vec<PairDisplay>),
    Shutdown,
}

pub struct TableDisplay {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    receiver: mpsc::Receiver<DisplayMessage>,
    show_logs: bool,
    tui_logger_state: tui_logger::TuiWidgetState,
}

impl TableDisplay {
    pub fn new(receiver: mpsc::Receiver<DisplayMessage>) -> Result<Self> {
        let backend = CrosstermBackend::new(io::stdout());
        let terminal = Terminal::new(backend)?;
        Ok(Self {
            terminal,
            receiver,
            show_logs: true,
            tui_logger_state: tui_logger::TuiWidgetState::new(),
        })
    }
    
    pub async fn start_display(&mut self) -> Result<()> {
        // 启用原始模式并进入备用屏幕
        terminal::enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen)?;
        
        let mut current_pairs = Vec::new();
        
        // 显示初始空表格
        self.terminal.draw(|f| Self::render_ui_static(f, &current_pairs))?;
        
        loop {
            tokio::select! {
                message = self.receiver.recv() => {
                    match message {
                        Some(DisplayMessage::UpdateData(pairs)) => {
                            current_pairs = pairs;
                            let _ = self.terminal.draw(|f| {
                                if self.show_logs {
                                     Self::render_ui_with_logs(f, &current_pairs, &mut self.tui_logger_state);
                                 } else {
                                     Self::render_ui_static(f, &current_pairs);
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
                                    let _ = self.terminal.draw(|f| {
                                        if self.show_logs {
                                             Self::render_ui_with_logs(f, &current_pairs, &mut self.tui_logger_state);
                                         } else {
                                             Self::render_ui_static(f, &current_pairs);
                                         }
                                    });
                                }
                                _ => {
                                    if self.show_logs {
                                        if let Some(tui_event) = self.map_key_to_tui_event(key.code) {
                                             self.tui_logger_state.transition(tui_event);
                                            let _ = self.terminal.draw(|f| {
                                                Self::render_ui_with_logs(f, &current_pairs, &mut self.tui_logger_state);
                                            });
                                        }
                                    }
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

    fn map_key_to_tui_event(&self, key_code: KeyCode) -> Option<tui_logger::TuiWidgetEvent> {
        use tui_logger::TuiWidgetEvent;
        match key_code {
            KeyCode::Up => Some(TuiWidgetEvent::UpKey),
            KeyCode::Down => Some(TuiWidgetEvent::DownKey),
            KeyCode::Left => Some(TuiWidgetEvent::LeftKey),
            KeyCode::Right => Some(TuiWidgetEvent::RightKey),
            KeyCode::Char('h') => Some(TuiWidgetEvent::HideKey),
            KeyCode::Char('f') => Some(TuiWidgetEvent::FocusKey),
            KeyCode::Char('+') => Some(TuiWidgetEvent::PlusKey),
            KeyCode::Char('-') => Some(TuiWidgetEvent::MinusKey),
            KeyCode::Char(' ') => Some(TuiWidgetEvent::SpaceKey),
            KeyCode::Esc => Some(TuiWidgetEvent::EscapeKey),
            _ => None,
        }
    }

    fn render_ui_with_logs(f: &mut Frame, pairs: &[PairDisplay], tui_logger_state: &mut tui_logger::TuiWidgetState) {
        // Split screen: table on top, logs on bottom
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(f.size());
        
        Self::render_table_area(f, chunks[0], pairs, true);
        Self::render_log_area(f, chunks[1], tui_logger_state);
    }

    fn render_log_area(f: &mut Frame, area: Rect, tui_logger_state: &mut tui_logger::TuiWidgetState) {
        let tui_logger_widget = TuiLoggerWidget::default()
            .block(Block::default()
                .title("日志 (按 'l' 切换显示)")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)))
            .output_separator('|')
            .output_timestamp(Some("%H:%M:%S%.3f".to_string()))
            .output_level(Some(TuiLoggerLevelOutput::Abbreviated))
            .output_target(true)
            .output_file(false)
            .output_line(false)
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
            let header_cells = ["排名", "交易对", "DEX", "价格 (USD)", "24h变化", "流动性", "最后更新"]
                .iter()
                .map(|h| Cell::from(*h).style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));
            let header = Row::new(header_cells).height(1).bottom_margin(1);
            
            let rows = pairs.iter().map(|pair| {
                let cells = vec![
                    Cell::from(pair.rank.to_string()),
                    Cell::from(pair.pair.clone()),
                    Cell::from(pair.dex.clone()),
                    Cell::from(pair.price.clone()),
                    Cell::from(pair.change_24h.clone()).style(
                        if pair.change_24h.starts_with('+') {
                            Style::default().fg(Color::Green)
                        } else {
                            Style::default().fg(Color::Red)
                        }
                    ),
                    Cell::from(pair.liquidity.clone()),
                    Cell::from(pair.last_update.clone()),
                ];
                Row::new(cells).height(1).bottom_margin(1)
            });
            
            let table = Table::new(rows, [
                Constraint::Length(4),  // 排名
                Constraint::Length(12), // 交易对
                Constraint::Length(10), // DEX
                Constraint::Length(12), // 价格
                Constraint::Length(10), // 24h变化
                Constraint::Length(12), // 流动性
                Constraint::Length(12), // 最后更新
            ])
            .header(header)
            .block(Block::default().borders(Borders::ALL).title("交易对数据"))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
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
            "按 Ctrl+C 退出 | 按 'l' 隐藏日志"
        } else {
            "按 Ctrl+C 退出 | 按 'l' 显示日志"
        };
        
        let help = Paragraph::new(help_text)
            .style(Style::default().fg(Color::Gray))
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(help, chunks[2]);
    }

    fn render_ui_static(f: &mut Frame, pairs: &[PairDisplay]) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3), // 标题
                Constraint::Min(0),    // 表格
                Constraint::Length(3), // 提示信息
            ])
            .split(f.size());
        
        // 渲染标题
        let title = Paragraph::new("🚀 实时交易对监控")
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(title, chunks[0]);
        
        // 渲染表格
        if !pairs.is_empty() {
            let header_cells = ["排名", "交易对", "DEX", "价格 (USD)", "24h变化", "流动性", "最后更新"]
                .iter()
                .map(|h| Cell::from(*h).style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)));
            let header = Row::new(header_cells).height(1).bottom_margin(1);
            
            let rows = pairs.iter().map(|pair| {
                let cells = vec![
                    Cell::from(pair.rank.to_string()),
                    Cell::from(pair.pair.clone()),
                    Cell::from(pair.dex.clone()),
                    Cell::from(pair.price.clone()),
                    Cell::from(pair.change_24h.clone()).style(
                        if pair.change_24h.starts_with('+') {
                            Style::default().fg(Color::Green)
                        } else {
                            Style::default().fg(Color::Red)
                        }
                    ),
                    Cell::from(pair.liquidity.clone()),
                    Cell::from(pair.last_update.clone()),
                ];
                Row::new(cells).height(1)
            });
            
            let table = Table::new(rows, &[
                Constraint::Length(4),  // 排名
                Constraint::Length(12), // 交易对
                Constraint::Length(12), // DEX
                Constraint::Length(12), // 价格
                Constraint::Length(8),  // 24h变化
                Constraint::Length(10), // 流动性
                Constraint::Length(10), // 最后更新
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
        let help = Paragraph::new("按 Ctrl+C 退出监控 | 数据实时更新中...")
            .style(Style::default().fg(Color::Yellow))
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(help, chunks[2]);
    }
}