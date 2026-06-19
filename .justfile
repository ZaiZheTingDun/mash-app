alias ss := screenshot

screenshot server file_name:
  @echo 'Taking screenshot on {{server}} into {{file_name}}'
  adb -s 127.0.0.1:5555 exec-out screencap -p > .screenshots/{{server}}/{{file_name}}.png