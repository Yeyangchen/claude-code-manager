; The stock Tauri uninstaller provides an unchecked Delete app data checkbox.
; Never remove Claude installations/configuration from a manager uninstall hook.
; Updating the manager must retain its operation ledger and skip this reminder.
LangString ccmRestoreNotice 2052 "卸载 Claude Code Manager 不会卸载 Claude Code，也不会自动撤销已写入的 PATH 或更新策略。$\r$\n$\r$\n若需还原，请点击取消，回到管理器的设置页逐项还原更改。$\r$\n$\r$\n默认保留管理器的数据与变更台账。若勾选删除应用数据，变更台账也会被删除。继续卸载？"
LangString ccmRestoreNotice 1033 "Uninstalling Claude Code Manager keeps Claude Code installed and does not automatically revert PATH or update policy changes.$\r$\n$\r$\nTo restore changes, cancel and use Settings in the manager first.$\r$\n$\r$\nManager data and the change ledger are kept by default. Selecting Delete app data also deletes the ledger. Continue uninstalling?"

!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
  ${AndIf} $PassiveMode <> 1
  ${AndIfNot} ${Silent}
    MessageBox MB_OKCANCEL|MB_ICONINFORMATION "$(ccmRestoreNotice)" /SD IDOK IDOK ccm_continue_uninstall
      Abort
    ccm_continue_uninstall:
  ${EndIf}
!macroend
