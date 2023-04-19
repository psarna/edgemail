const PAGE_SIZE = 5;
const urlParams = new URLSearchParams(window.location.search);
const user = urlParams.get('user');
const offset = parseInt(urlParams.get('offset')) || 0;
document.getElementById('title').innerHTML = user + "@idont.date";

if (offset > 0) {
    const prev = document.getElementById('prev');
    prev.onclick = () => {
        window.location.href = './inbox.html?user=' + user + '&offset=' + Math.max(0, offset - PAGE_SIZE);
    }
    prev.disabled = false;
}
document.getElementById('current_page').innerHTML = "page " + Math.floor(offset / PAGE_SIZE + 1);

const req = new XMLHttpRequest();
const url = 'https://spin-psarna.turso.io';
req.open("POST", url);
const readonly_token = "eyJhbGciOiJFZERTQSIsInR5cCI6IkpXVCJ9.eyJhIjoicm8iLCJpYXQiOjE2ODE4MjkxNDMsImlkIjoiNzIyY2IyYTEtY2M3MC0xMWVkLWFkM2MtOGVhNWEwNjcyYmM2In0.T55UgAMs9vP2zMI_AhOiD2AONj_bsnDNRjZiBBWUb2gKU5MEjJoW8uHbtMGqpJ0312SULpsWTWdEJ886oSjGCQ";
req.setRequestHeader('Authorization', 'Bearer ' + readonly_token)

req.send(JSON.stringify({ statements: [{ q: "SELECT date, sender, recipients, data FROM mail WHERE recipients = ? ORDER BY ROWID DESC LIMIT ? OFFSET ?", params: ["<" + user + "@idont.date>", PAGE_SIZE, offset] }] }));

// Some of these rules are heavily inspired by https://www.npmjs.com/package/quoted-printable:
// FIXME: proper sanitizer should read the encoding from the headers or deduce it,
// and then apply it accordingly.

function sanitize(s) {
    return s
        .replaceAll('=E2=80=8A', '')
        .replaceAll('=E2=80=8B', '')
        .replaceAll('=E2=80=8C', '')
        .replaceAll('=C2=A0', '<br>')
        .replaceAll('=E2=80=99', "'")
        .replaceAll(/[\t\x20]$/gm, '')
        // Remove hard line breaks preceded by `=`. Proper `Quoted-Printable`-
        // encoded data only contains CRLF line  endings, but for compatibility
        // reasons we support separate CR and LF too.
        .replaceAll(/=(?:\r\n?|\n|$)/g, '')
        // Decode escape sequences of the form `=XX` where `XX` is any
        // combination of two hexidecimal digits. For optimal compatibility,
        // lowercase hexadecimal digits are supported as well. See
        // https://tools.ietf.org/html/rfc2045#section-6.7, note 1.
        .replaceAll(/=([a-fA-F0-9]{2})/g, function (_match, target) {
            var codePoint = parseInt(target, 16);
            return String.fromCharCode(codePoint);
        });
}

function parse(email) {
    const subject_position = email.indexOf('Subject: ') || email.indexOf('SUBJECT: ');
    let subject = email.substring(subject_position + 9, email.indexOf('\r\n', subject_position));
    if (subject.toLowerCase().startsWith("=?utf-8?")) {
        subject = sanitize(subject.substring(10));
    }

    const from_position = email.indexOf('From: ') || email.indexOf('FROM: ');
    let from = email.substring(from_position + 6, email.indexOf('\r\n', from_position))
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;");
    if (from.toLowerCase().startsWith("=?utf-8?")) {
        from = sanitize(from.substring(10));
    }

    const body_position = email.indexOf('<body') || email.indexOf('<BODY') || email.indexOf('\r\n\r\n');
    const body = sanitize(email.substring(body_position))

    return [from, subject, body];
}

function createTable(data) {
    const table = document.createElement('table');
    table.className = 'table-hover';
    table.style.border = '1px solid';
    const thead = document.createElement('thead');
    const tbody = document.createElement('tbody');
    const tr = document.createElement('tr');
    const th1 = document.createElement('th');
    const th2 = document.createElement('th');
    const th3 = document.createElement('th');
    th1.style.border = th2.style.border = th3.style.border = '1px solid';
    th1.innerHTML = "from";
    th2.innerHTML = "subject";
    th3.innerHTML = "date";
    tr.appendChild(th1);
    tr.appendChild(th2);
    tr.appendChild(th3);
    thead.appendChild(tr);
    table.appendChild(thead);
    for (const row of data.rows) {
        const tr = document.createElement('tr');
        const td1 = document.createElement('td');
        const td2 = document.createElement('td');
        const td3 = document.createElement('td');
        td1.style.border = td2.style.border = td3.style.border = '1px solid';
        const [from, subject, body] = parse(row[3]);
        td1.innerHTML = from || row[1].slice(1, -1);
        td2.innerHTML = subject || "[no subject]";
        td3.innerHTML = new Date(row[0]).toLocaleString();
        tr.appendChild(td1);
        tr.appendChild(td2);
        tr.appendChild(td3);
        tr.onclick = () => {
            document.getElementById('datapanel').innerHTML = body;
        }
        tbody.appendChild(tr);
    }
    if (data.rows.length == 0) {
        const tr = document.createElement('tr');
        const div = document.createElement('div');
        div.style.textAlign = 'center';
        const h = document.createElement('h4');
        h.innerHTML = "No e-mails for " + user + "@idont.date yet! <br> &#8635; refresh";
        h.onclick = _ => window.location.reload();
        div.appendChild(h);
        tr.appendChild(div);
        tbody.appendChild(tr);
    }
    table.appendChild(tbody);
    return table;
}

req.onload = (e) => {
    if (req.status != 200) {
        const msg = document.createElement('p');
        msg.style.textAlign = 'center';
        msg.innerText = "Error: " + req.responseText;
        document.getElementById('inbox_table').appendChild(msg);
        return;
    }
    const response = JSON.parse(req.responseText);
    document.getElementById('inbox_table').appendChild(createTable(response[0].results))
    if (response[0].results.rows.length >= PAGE_SIZE) {
        const next = document.getElementById('next');
        next.onclick = () => {
            window.location.href = './inbox.html?user=' + user + '&offset=' + (offset + PAGE_SIZE);
        };
        next.disabled = false;
    }
}
