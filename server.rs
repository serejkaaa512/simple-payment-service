use rand;
use std::io::Write;
use nickel::status::StatusCode::NotFound;
use nickel::{Nickel, NickelError, Continue, Halt, Request, Response, MediaType, QueryString,
             ListeningServer, JsonBody, MiddlewareResult, HttpRouter, Action};
use rustc_serialize::json;
use std::sync::{Arc, Mutex};
use std::error::Error as StdError;

#[derive(RustcDecodable, RustcEncodable, Debug, Clone)]
struct Person {
    firstname: String,
    lastname: String,
}

#[derive(RustcDecodable, RustcEncodable, Debug, Clone)]
struct Payment {
    customer: Person,
    account: String,
    amount: f64,
}

#[derive(RustcDecodable, RustcEncodable, Debug, Clone)]
struct AccountInfo {
    customer: Person,
    account: String,
    amount: f64,
}

#[derive(Debug)]
struct Bank {
    accounts: Vec<AccountInfo>,
}

impl Bank {
    fn new() -> Bank {
        Bank { accounts: vec![] }
    }

    fn add_customer(&mut self, customer: Person) -> String {
        let account = rand::random::<i32>().abs().to_string();
        let amount = 0f64;
        let acc = AccountInfo {
            customer: customer,
            account: account.clone(),
            amount: amount,
        };
        self.accounts.push(acc.clone());
        json::encode(&acc).unwrap()
    }

    fn add_payment(&mut self, pay: Payment) -> String {
        let ac = self.accounts
            .iter_mut()
            .find(|ac| {
                ac.customer.firstname == pay.customer.firstname &&
                ac.customer.lastname == pay.customer.lastname &&
                ac.account == pay.account
            })
            .unwrap();
        ac.amount += pay.amount;
        format!("Payment received. New data: Customer - {} {}. Account - {}. Amount - {}",
                ac.customer.firstname,
                ac.customer.lastname,
                ac.account,
                ac.amount)
    }

    fn get_account_info(&mut self, customer: Person) -> String {
        let ac = self.accounts
            .iter()
            .find(|ac| {
                ac.customer.firstname == customer.firstname &&
                ac.customer.lastname == customer.lastname
            })
            .unwrap();
        json::encode(&ac).unwrap()
    }
}



fn custom_404<'a, D>(err: &mut NickelError<D>, _req: &mut Request<D>) -> Action {
    if let Some(ref mut res) = err.stream {
        if res.status() == NotFound {
            let _ = res.write_all(b"<h1>Call the police!</h1>");
            return Halt(());
        }
    }

    Continue(())
}



// curl 'http://localhost:6767/customers' -H 'Content-Type: application/json;charset=UTF-8'  --data-binary $'{ "firstname": "John","lastname": "Lock" }'
fn post_customers<'mw>(req: &mut Request<Arc<Mutex<Bank>>>,
                       mut res: Response<'mw, Arc<Mutex<Bank>>>)
                       -> MiddlewareResult<'mw, Arc<Mutex<Bank>>> {
    let mut my_bank = req.server_data().lock().unwrap();
    let customer = req.json_as::<Person>().unwrap();
    let output = my_bank.add_customer(customer);
    res.set(MediaType::Json);
    res.send(output)
}

// http://localhost:6767/balance?firstname=John&lastname=Lock
fn get_balance<'mw>(req: &mut Request<Arc<Mutex<Bank>>>,
                    mut res: Response<'mw, Arc<Mutex<Bank>>>)
                    -> MiddlewareResult<'mw, Arc<Mutex<Bank>>> {
    let mut my_bank = req.server_data().lock().unwrap();
    let query = req.query();
    let firstname = query.get("firstname").unwrap();
    let lastname = query.get("lastname").unwrap();
    let customer = Person {
        firstname: firstname.clone().to_string(),
        lastname: lastname.clone().to_string(),
    };
    res.set(MediaType::Json);
    let balance = my_bank.get_account_info(customer);
    res.send(balance)
}

// curl 'http://localhost:6767/pay' -H 'Content-Type: application/json;charset=UTF-8'  --data-binary $'{ "person": { "firstname": "John","lastname": "Lock" }, "account": "1321321321312", "amount": "80"}'
fn post_pay<'mw>(req: &mut Request<Arc<Mutex<Bank>>>,
                 res: Response<'mw, Arc<Mutex<Bank>>>)
                 -> MiddlewareResult<'mw, Arc<Mutex<Bank>>> {
    let payment = req.json_as::<Payment>().unwrap();
    let mut my_bank = req.server_data().lock().unwrap();
    let output = my_bank.add_payment(payment);
    res.send(output)
}



pub fn create_server(address: &str) -> Result<ListeningServer, Box<StdError>> {
    let my_bank = Arc::new(Mutex::new(Bank::new()));
    let custom_handler: fn(&mut NickelError<Arc<Mutex<Bank>>>,
                           &mut Request<Arc<Mutex<Bank>>>)
                           -> Action = custom_404;
    let mut server = Nickel::with_data(my_bank);
    server.get("/balance", get_balance)
        .post("/customers", post_customers)
        .post("/pay", post_pay)
        .handle_error(custom_handler);

    server.listen(address)
}




#[cfg(test)]
mod tests {

    use self::support::{Body, get, post};
    use hyper::header;
    use nickel::status::StatusCode;
    use rustc_serialize::json::Json;


    #[ignore]
    #[test]
    fn post_customer() {
        // create simple man
        let mut response = post("/customers",
                                r#"{ "firstname": "John", "lastname": "Lock" }"#);
        let json = Json::from_str(&response.body()).unwrap();


        assert_eq!(response.status, StatusCode::Ok);
        assert_eq!(response.headers.get::<header::ContentType>(),
                   Some(&header::ContentType::json()));
        assert_eq!(json["person"]["firstname"].as_string(), Some("John"));
        assert_eq!(json["person"]["lastname"].as_string(), Some("Lock"));
    }

    #[ignore]
    #[test]
    fn get_balance() {
        // create simple man
        post("/customers",
             r#"{ "firstname": "John", "lastname": "Lock" }"#);

        // ask for simple man account
        let mut response = get("/balance?firstname=John&lastname=Lock");
        let json = Json::from_str(&response.body()).unwrap();


        assert_eq!(response.status, StatusCode::Ok);
        assert_eq!(response.headers.get::<header::ContentType>(),
                   Some(&header::ContentType::json()));
        assert_eq!(json["customer"]["firstname"].as_string(), Some("John"));
        assert_eq!(json["customer"]["lastname"].as_string(), Some("Lock"));
        assert_eq!(json["amount"].as_f64(), Some(0f64));
    }

    #[ignore]
    #[test]
    fn post_pay() {
        // create simple man
        let mut response = post("/customers",
                                r#"{ "firstname": "John", "lastname": "Lock" }"#);
        let json = Json::from_str(&response.body()).unwrap();
        let account = json["account"].as_string().unwrap();

        let json_send = format!("{}{}{}",
                    r#"{ "customer": { "firstname": "John","lastname": "Lock" }, "account": ""#,
                    &*account,
                    r#"", "amount": "80"}"#);

        // simple man pay
        response = post("/pay", &*json_send);
        let answer = format!("{}{}{}",
                             "Payment received. New data: Customer - John Lock. Account - ",
                             &*account,
                             ". Amount - 80");

        assert_eq!(response.status, StatusCode::Ok);

        assert_eq!(response.body(), answer);

    }



    #[test]
    fn post_pay_to_hacker_check() {
        use std::thread;
        use std::time::Duration;

        // create hacker
        post("/customers",
             r#"{ "firstname": "SUPER", "lastname": "HACKER" }"#);

        // create simple man
        let mut response = post("/customers",
                                r#"{ "firstname": "John", "lastname": "Lock" }"#);


        let json = Json::from_str(&response.body()).unwrap();
        let account = json["account"].as_string().unwrap();
        let json_send = format!("{}{}{}",
                    r#"{ "customer": { "firstname": "John","lastname": "Lock" }, "account": ""#,
                    &*account,
                    r#"", "amount": "80"}"#);
        // simple man pay
        post("/pay", &*json_send);

        thread::sleep(Duration::from_millis(1000));

        // ask for hacker account
        response = get("/balance?firstname=SUPER&lastname=HACKER");
        let json = Json::from_str(&response.body()).unwrap();
        let hacker_amount = json["amount"].as_f64();
        assert_eq!(hacker_amount, Some(0.00001f64));
    }



    mod support {
        use hyper::client::{Client, Response as HyperResponse};
        use nickel::ListeningServer;

        use std::net::SocketAddr;

        pub trait Body {
            fn body(self) -> String;
        }

        impl<'a> Body for &'a mut HyperResponse {
            fn body(self) -> String {
                use std::io::Read;
                let mut body = String::new();
                self.read_to_string(&mut body).expect("Failed to read body of Response");
                body
            }
        }

        pub struct Server(SocketAddr);
        impl Server {
            pub fn new(server: ListeningServer) -> Server {
                let wrapped = Server(server.socket());

                server.detach();

                wrapped
            }

            pub fn get(&self, path: &str) -> HyperResponse {
                let url = self.url_for(path);
                Client::new().get(&url).send().unwrap()
            }

            pub fn post(&self, path: &str, body: &str) -> HyperResponse {
                let url = self.url_for(path);
                Client::new().post(&url).body(body).send().unwrap()
            }

            pub fn url_for(&self, path: &str) -> String {
                format!("http://{}{}", self.0, path)
            }
        }

        lazy_static! {
            pub static ref STATIC_SERVER: Server = {
                let server = super::super::create_server("127.0.0.1:6767").unwrap();
                Server::new(server)
            };
        }

        pub fn get(path: &str) -> HyperResponse {
            STATIC_SERVER.get(path)
        }

        pub fn post(path: &str, body: &str) -> HyperResponse {
            STATIC_SERVER.post(path, body)
        }
    }
}