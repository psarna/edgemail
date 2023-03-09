import smtplib
import sys

fromaddr = "testfrom@example.com"
toaddr  = "testto@example.com"

# Add the From: and To: headers at the start!
msg = f"From: {fromaddr}\r\nTo: {toaddr}\r\n\r\n"
msg += "test \nmail\n goodbye\n"

if len(sys.argv) < 3:
    print(f"Usage: {sys.argv[0]} HOST PORT")
    exit(1)

server = smtplib.SMTP(sys.argv[1], port=sys.argv[2])
server.set_debuglevel(1)
server.sendmail(fromaddr, toaddr, msg)
server.quit()
