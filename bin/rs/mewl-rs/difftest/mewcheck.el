;;; mewcheck.el --- decode encoded-words as Mew does, for mewcheck.py
;;
;; Reads lines "charset<TAB>base64" from stdin and prints, for each, the
;; code points of what mew-header-decode makes of =?charset?B?base64?=,
;; space-separated in hex, with "R" for a raw byte (invalid input), "X"
;; for a character without Unicode, and "U" if the charset is unknown.
;; With the argument "charsets", prints the charsets Mew knows instead.

(require 'mew)

(defun mewcheck-code (c)
  (let ((u (encode-char c 'unicode)))
    (cond
     ((eq (char-charset c) 'eight-bit) "R")
     ((null u) "X")
     (t (format "%X" u)))))

(if (member "charsets" command-line-args-left)
    (dolist (db mew-cs-database-for-decoding)
      (princ (format "%s\t%s\n" (car db) (or (cadr db) "nil"))))
  (let (line)
    (while (setq line (ignore-errors (read-from-minibuffer "")))
      (let* ((fields (split-string line "\t"))
             (str (mew-header-decode (car fields) "B" (cadr fields))))
        ;; A charset not known is guessed (mew-use-autoconv-when-unknown).
        (if (equal str mew-error-unknown-charset)
            (princ "U")
          (princ (mapconcat #'mewcheck-code str " ")))
        (terpri)))))
(setq command-line-args-left nil)
