"""One full, held loopback response. Old three-request server stays unchanged."""
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import re
import threading
from original_download_native import BYTES, CHUNK
from save_workflow_contracts import require, identical


class WorkflowOriginal:
    def __init__(self, nonce):
        require(re.fullmatch('[0-9a-f]{32}', nonce), 'nonce')
        self.nonce, self.rows, self.lock = nonce, [], threading.Lock()
        self.gate = threading.Event()
        self.handlers = []
        self.connections = 0
        owner = self

        class Handler(BaseHTTPRequestHandler):
            def setup(self):
                super().setup()
                self.connection.settimeout(5)

            def log_message(self, *_):
                pass

            def parse_request(self):
                result = super().parse_request()
                if result and self.command != 'GET':
                    with owner.lock:
                        if len(owner.rows) < 4:
                            owner.rows.append(dict(error='unexpected method', method=self.command[:64]))
                    self.send_error(405)
                    return False
                return result

            def do_GET(self):
                self.connection.settimeout(10)
                with owner.lock:
                    if owner.rows:
                        if len(owner.rows) < 4:
                            owner.rows.append(dict(error='extra request'))
                        self.send_error(400)
                        return
                    row = dict(index=1, method='GET', path=self.path,
                               cookieAbsent=self.headers.get('Cookie') is None,
                               authorizationAbsent=self.headers.get('Authorization') is None,
                               encoding=self.headers.get('Accept-Encoding'), bytesSent=0, closed=False)
                    owner.rows.append(row)
                try:
                    require(row['path'] == '/original/' + nonce and row['cookieAbsent']
                            and row['authorizationAbsent'] and row['encoding'] == 'identity', 'unexpected original request')
                    self.send_response(200)
                    self.send_header('Content-Length', str(BYTES))
                    self.send_header('Content-Type', 'application/octet-stream')
                    self.end_headers()
                    self.wfile.write(CHUNK)
                    self.wfile.flush()
                    row['bytesSent'] = len(CHUNK)
                    require(owner.gate.wait(45), 'held body expired')
                    remaining = BYTES - len(CHUNK)
                    while remaining:
                        block = CHUNK[:min(len(CHUNK), remaining)]
                        self.wfile.write(block)
                        row['bytesSent'] += len(block)
                        remaining -= len(block)
                    self.wfile.flush()
                except BaseException as error:
                    row['error'] = f'{type(error).__name__}: {error}'
                finally:
                    row['closed'] = True
                    self.close_connection = True

        class Server(ThreadingHTTPServer):
            daemon_threads = True

            def verify_request(self, request, client_address):
                with owner.lock:
                    owner.connections += 1
                    if owner.connections > 4:
                        if not any(row.get('error') == 'connection budget exceeded' for row in owner.rows):
                            owner.rows.append(dict(error='connection budget exceeded'))
                        return False
                return True

            def process_request_thread(self, *args):
                with owner.lock:
                    owner.handlers.append(threading.current_thread())
                super().process_request_thread(*args)

        self.server = Server(('127.0.0.1', 0), Handler)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        try:
            self.thread.start()
        except BaseException:
            self.server.server_close()
            raise

    @property
    def port(self):
        return self.server.server_address[1]

    def release(self):
        self.gate.set()

    def close(self):
        self.release()
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=5)
        require(not self.thread.is_alive(), 'server thread exit')
        for thread in self.handlers:
            thread.join(timeout=12)
        require(all(not thread.is_alive() for thread in self.handlers), 'request handler exit')

    def validate(self):
        expected = [dict(index=1, method='GET', path='/original/' + self.nonce,
                         cookieAbsent=True, authorizationAbsent=True, encoding='identity',
                         bytesSent=BYTES, closed=True)]
        require(self.connections == 1 and identical(self.rows, expected), 'exact single connection/full unauthenticated GET')
        return list(self.rows)
