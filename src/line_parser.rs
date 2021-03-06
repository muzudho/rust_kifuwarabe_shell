use diagram::*;
/// 行単位のパーサー
use diagram_player::*;
use regex::Regex;
use shell::*;

/// 不具合を取りたいときに真にする。
const VERBOSE: bool = false;

pub struct LineParser {}
impl LineParser {
    /// 行単位パーサー。
    ///
    /// # Arguments.
    ///
    /// * `diagram` - この関数の中では、 Diagram をイミュータブルに保つ。 Diagram の編集は この関数の外で行う。
    /// * `req` - １行分だけ切り取ってテキストを送り返してくる。行をまたいでマッチできない。トークンに分解して送ってくることもできない。
    ///
    /// # Returns.
    ///
    /// 0. シェルを終了するなら真。
    pub fn run<T>(
        diagram_player: &mut DiagramPlayer,
        diagram: &Diagram<T>,
        t: &mut T,
        req: &mut dyn Request,
        res: &mut dyn Response,
    ) {
        // 現在地が遷移図の外なら、入り口から入れだぜ☆（＾～＾）
        diagram_player.enter_when_out(&diagram);

        // レスポンスを、デフォルト値にリセット。
        if let Some(res) = res.as_mut_any().downcast_mut::<ResponseStruct>() {
            res.reset();
        } else {
            panic!("Downcast fail. res.");
        }

        'line: while req.get_caret() < req.get_line_len() {
            // リクエストとレスポンスをクリアー。
            if let Some(req) = req.as_mut_any().downcast_mut::<RequestStruct>() {
                req.groups.clear(); // クリアー
            } else {
                panic!("Downcast fail. req.");
            }
            // キャレットの位置を、レスポンスからリクエストへ移して、次のトークンへ。
            // レスポンスはリセットせず、前のループの内容を使いまわす。

            // ****************************************************************************************************
            // * 次の行き先に遷移。（フォワード）                                                             *
            // ****************************************************************************************************
            let best_is_regex = if let Some(res) = res.as_any().downcast_ref::<ResponseStruct>() {
                diagram_player.forward_parse(diagram, req, &res.exit_label.to_string())
            } else {
                panic!("Downcast fail.");
            };

            // キャレットを進める。
            if diagram_player.get_current() != "" {
                res.set_caret(req.get_caret());

                if best_is_regex {
                    // 正規表現に一致なら
                    LineParser::parse_reg(req, res);

                } else {
                    LineParser::parse_literal(&diagram.get_node(&diagram_player.get_current()), req, res);

                }

                if let Some(req) = req.as_mut_any().downcast_mut::<RequestStruct>() {
                    if let Some(res) = res.as_any().downcast_ref::<ResponseStruct>() {
                        req.caret = res.caret;
                    } else {
                        panic!("Downcast fail.");
                    }
                } else {
                    panic!("Downcast fail.");
                }

                // res.set_caret(0);
                res.set_caret(req.get_caret());
                res.forward(NEXT_EXIT_LABEL); // デフォルト値。

                let node = &diagram.get_node(&diagram_player.get_current());

                // あれば、コントローラーに処理を移譲。
                if node.get_fn_label() == "" {
                    // コントローラーを指定していなければ、出口ラベルは、デフォルト値のまま。
                } else if diagram.contains_fn(&node.get_fn_label()) {
                    (diagram.get_fn(&node.get_fn_label()))(t, req, res);
                } else {
                    // 無い関数が設定されていた場合は、コンソール表示だけする。
                    println!(
                        "IGNORE: \"{}\" fn (in {} node) is not found.",
                        &node.get_fn_label(),
                        diagram_player.get_current()
                    );
                }

                if let Some(res) = res.as_any().downcast_ref::<ResponseStruct>() {
                    if res.done_line {
                        // 行解析の終了。
                        let len = req.get_line_len();

                        if let Some(req) = req.as_mut_any().downcast_mut::<RequestStruct>() {
                            req.caret = len;
                        } else {
                            panic!("Downcast fail.");
                        }
                    }
                } else {
                    panic!("Downcast fail.");
                }
            } else {
                // 何とも一致しなかったら実行します
                LineParser::parse_line_else(&diagram, t, req, res);
                // 次のラインへ。
                break 'line;
            }

            if let Some(res) = res.as_any().downcast_ref::<ResponseStruct>() {
                use diagram::ResponseOption::*;
                match res.option {
                    None => {}
                    Quits => {
                        return;
                    }
                    Reloads(ref _file) => {
                        return;
                    }
                    Saves(ref _file) => {
                        return;
                    }
                }
            } else {
                panic!("Downcast fail.");
            }

            // 次のトークンへ。
        }

        // ここで、現在ノードは "#newline" を記述している必要がある。
        // ****************************************************************************************************
        //  (指定があるなら)行終了を「登録」。(行終了するわけではない)
        // ****************************************************************************************************
        let node = &diagram.get_node(&diagram_player.get_current());
        if node.contains_exit(&NEWLINE_EXIT_LABEL.to_string()) {
            // 次の「行末」ノードへ。抽出するノード ラベルは 必ず先頭の1つだけ とする。
            let tail_node_label = &node.get_exit_vec(&NEWLINE_EXIT_LABEL.to_string())[0];

            // 「行末」の関数を「登録」する。
            let tail_node = diagram.get_node(&tail_node_label);
            let fn_label = tail_node.get_fn_label();
            if diagram.contains_fn(&fn_label) {
                let mut current_newline_fn: Controller<T> = *diagram.get_fn(&fn_label);
                // ****************************************************************************************************
                //  改行（1行読取）に対応したコールバック関数を実行。
                // ****************************************************************************************************
                (current_newline_fn)(t, req, res); // responseは無視する。

            } else {
                // 無い関数が設定されていた場合は、コンソール表示だけする。
                println!(
                    "IGNORE: \"{}\" fn (in {} node) is not found.",
                    &fn_label, NEWLINE_EXIT_LABEL
                );
            }

            // 次の「行頭」ノードを「登録」。抽出するノード ラベルは 必ず先頭の1つだけ とする。
            let mut registered_next_head_node_label =
                tail_node.get_exit_vec(NEXT_EXIT_LABEL)[0].to_string();
            diagram_player.set_current(&registered_next_head_node_label);
            /*
            println!(
                "行終了登録 tail_node_label: [{}], registered_next_head_node_label: [{}].",
                tail_node_label, registered_next_head_node_label
            );
                */


            // TODO 改行の設定、廃止したい。
            // if registered_next_head_node_label != "" {
                // 設定されているなら、上書き。
            //}
            /*
            println!(
                "行終了 self.current_label: [{}].",
                self.current_label
            );
            */

        } else {
            panic!("\"#newline\" door is not found. (current [{}] node)", diagram_player.get_current());
        }
    }

    // cyclomatic complexity を避けたいだけ。
    pub fn parse_line_else<T>(
        diagram: &Diagram<T>,
        t: &mut T,
        req: &mut dyn Request,
        res: &mut dyn Response,
    ) {
        if diagram.contains_node(&ELSE_NODE_LABEL.to_string()) {
            let fn_label = diagram.get_node(&ELSE_NODE_LABEL.to_string())
                .get_fn_label();
            if diagram.contains_fn(&fn_label) {
                // ****************************************************************************************************
                //  コールバック関数を実行。
                // ****************************************************************************************************
                (diagram.get_fn(&fn_label))(t, req, res);
            // responseは無視する。
            } else {
                // 無い関数が設定されていた場合は、コンソール表示だけする。
                println!(
                    "IGNORE: \"{}\" fn (in {} node) is not found.",
                    &fn_label, ELSE_NODE_LABEL
                );
            }
        }
    }

    fn parse_literal(node: &Node, req: &dyn Request, res: &mut dyn Response) {
        res.set_caret(req.get_caret() + node.get_token().len());
        let res_caret;
        if let Some(res) = res.as_any().downcast_ref::<ResponseStruct>() {
            res_caret = res.caret;
        } else {
            panic!("Downcast fail.");
        }

        // 続きにスペース「 」が１つあれば読み飛ばす
        if 0 < (req.get_line_len() - res_caret)
            && &req.get_line()[res_caret..(res_caret + 1)] == " "
        {
            res.set_caret(res_caret + 1);
        }
    }

    /// TODO キャレットを進める。正規表現はどこまで一致したのか分かりにくい。
    fn parse_reg(req: &dyn Request, res: &mut dyn Response) {
        // グループ[0]の文字数で取る。
        let pseud_token_len = req.get_groups()[0].chars().count();
        res.set_caret(req.get_caret() + pseud_token_len);

        // 続きにスペース「 」が１つあれば読み飛ばす
        let res_caret;
        if let Some(res) = res.as_any().downcast_ref::<ResponseStruct>() {
            res_caret = res.caret;
        } else {
            panic!("Downcast fail.");
        }
        if 0 < (req.get_line_len() - res_caret)
            && &req.get_line()[res_caret..(res_caret + 1)] == " "
        {
            res.set_caret(res_caret + 1);
        }
    }

    /// [token]文字列の長さだけ [starts]キャレットを進めます。
    /// [token]文字列の続きに半角スペース「 」が１つあれば、1つ分だけ読み進めます。
    ///
    /// # Arguments
    ///
    /// * `req` - 読み取るコマンドラインと、読取位置。
    /// * returns - 一致したら真。
    pub fn starts_with_literal(node: &Node, req: &dyn Request) -> bool {
        let caret_end = req.get_caret() + node.get_token().len();
        caret_end <= req.get_line_len()
            && &req.get_line()[req.get_caret()..caret_end] == node.get_token()
    }

    /// 正規表現を使う。
    ///
    /// # Arguments
    ///
    /// * `req` - 読み取るコマンドライン。一致があると groups メンバーに入れる。
    /// * returns - 一致したら真。
    pub fn starts_with_reg(node: &Node, req: &mut dyn Request) -> bool {
        if VERBOSE {
            println!("Starts_with_re");
        }

        if req.get_caret() < req.get_line_len() {
            if VERBOSE {
                println!("node.token: {}", node.get_token());
            }

            let re = Regex::new(&node.get_token()).unwrap();

            let text;
            let mut group_num = 0;
            if let Some(req) = req.as_mut_any().downcast_mut::<RequestStruct>() {
                text = &req.line[req.caret..];

                if VERBOSE {
                    println!("text: [{}]", text);
                }

                for caps in re.captures_iter(text) {
                    // caps は サイズ 2 の配列 で同じものが入っている。
                    let cap = &caps[0];

                    req.groups.push(cap.to_string());

                    group_num += 1;
                }

                if VERBOSE {
                    println!("Group num: {}", group_num);
                }
            } else {
                panic!("Downcast fail.");
            }

            0 < group_num
        } else {
            false
        }
    }
}
