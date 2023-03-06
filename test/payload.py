import smtplib

fromaddr = "testfrom@example.com"
toaddr  = "testto@example.com"

# Add the From: and To: headers at the start!
msg = f"From: {fromaddr}\r\nTo: {toaddr}\r\n\r\n"
msg += "test \nmail\n goodbye\n"

server = smtplib.SMTP('2a09:8280:1::1:f488', port=587)
server.set_debuglevel(1)
server.sendmail(fromaddr, toaddr, msg)
server.quit()
