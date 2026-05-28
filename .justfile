alias ss := screenshot

screenshot server file_name:
  @echo 'Taking screenshot on {{server}} into {{file_name}}'
  adb exec-out screencap -p > .screenshots/{{server}}/{{file_name}}.png