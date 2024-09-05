/* eslint-disable node/prefer-global/process */
/* eslint-disable no-console */
import * as os from 'node:os'
import * as path from 'node:path'
import type { ConfigurationChangeEvent, ExtensionContext, OutputChannel, WorkspaceConfiguration } from 'vscode'
import { Uri, commands, window, workspace } from 'vscode'

import type {
  Executable,
  LanguageClientOptions,
  ServerOptions,
} from 'vscode-languageclient/node'
import {
  LanguageClient,
} from 'vscode-languageclient/node'
import { getWorkspace } from 'ultra-runner'

let client: LanguageClient | undefined

export async function activate(
  context: ExtensionContext,
): Promise<void> {
  const name = 'TypedKey'

  const outputChannel = window.createOutputChannel(name)

  // context.subscriptions holds the disposables we want called
  // when the extension is deactivated
  context.subscriptions.push(outputChannel)

  context.subscriptions.push(
    workspace.onDidChangeConfiguration(
      async (e: ConfigurationChangeEvent) => {
        const restartTriggeredBy = [
          'typedkey.translationsDir',
          'typedkey.logLevel',
          'typedkey.path',
        ].find(s => e.affectsConfiguration(s))

        if (restartTriggeredBy) {
          await commands.executeCommand('typedkey.restart')
        }
      },
    ),
  )

  context.subscriptions.push(
    commands.registerCommand('typedkey.restart', async () => {
      // can't stop if the client has previously failed to start
      if (client && client.needsStop()) {
        await client.stop()
      }

      try {
        client = await createClient(context, name, outputChannel)
      }
      catch (err) {
        const msg = err instanceof Error ? err.message : ''
        window.showErrorMessage(`${msg}`)
        return
      }

      // Start the client. This will also launch the server
      await client.start()
    }),
  )

  // use the command as our single entry point for (re)starting
  // the client and server. This ensures at activation time we
  // start and handle errors in a way that's consistent with the
  // other triggers
  await commands.executeCommand('typedkey.restart')
}

async function createClient(
  context: ExtensionContext,
  name: string,
  outputChannel: OutputChannel,
): Promise<LanguageClient> {
  const env = { ...process.env }

  const config = workspace.getConfiguration('typedkey')
  const path = await getServerPath(context, config)

  outputChannel.appendLine(`Using typedkey server ${path}`)

  env.RUST_LOG = config.get('logLevel')

  const run: Executable = {
    command: path,
    options: { env },
  }

  const serverOptions: ServerOptions = {
    run,
    // used when launched in debug mode
    debug: run,
  }
  const resolvedTranslationsDir = await resolveTranslationsDir(
    config.get('translationsDir'),
  )

  const clientOptions: LanguageClientOptions = {
    // Register the server for all documents
    documentSelector: [
      { scheme: 'untitled' },
      { scheme: 'file', pattern: '**' },
      // source control commit message
      { scheme: 'vscode-scm' },
    ],
    outputChannel,
    traceOutputChannel: outputChannel,
    initializationOptions: {
      config: {
        translations_dir: resolvedTranslationsDir,
      },
    },
  }

  return new LanguageClient(
    name.toLowerCase(),
    name,
    serverOptions,
    clientOptions,
  )
}

async function getServerPath(
  context: ExtensionContext,
  config: WorkspaceConfiguration,
): Promise<string> {
  let path
    = process.env.TYPED_KEY_LSP_PATH ?? config.get<null | string>('path')

  if (path) {
    if (path.startsWith('~/')) {
      path = os.homedir() + path.slice('~'.length)
    }
    const pathUri = Uri.file(path)

    return await workspace.fs.stat(pathUri).then(
      () => pathUri.fsPath,
      () => {
        throw new Error(
          `${path} does not exist. Please check typedkey.path in Settings.`,
        )
      },
    )
  }

  // if (config.package.releaseTag === null) return "typed-key";

  const ext = process.platform === 'win32' ? '.exe' : ''
  const bundled = Uri.joinPath(
    context.extensionUri,
    'bundled',
    `typed-key${ext}`,
  )

  return await workspace.fs.stat(bundled).then(
    () => bundled.fsPath,
    () => {
      throw new Error(
        `Unfortunately we don't ship binaries for your platform yet. ${bundled.toString()} `
        + 'Try specifying typedkey.path in Settings. '
        + 'Or raise an issue [here](https://github.com/igorsheg/typed-key/issues) '
        + 'to request a binary for your platform.',
      )
    },
  )
}

export function deactivate(): Thenable<void> | undefined {
  if (!client) {
    return undefined
  }
  return client.stop()
}

async function resolveTranslationsDir(
  configPath: string | undefined,
): Promise<string | undefined> {
  if (!configPath) {
    return undefined
  }

  const workspaceFolders = workspace.workspaceFolders
  if (!workspaceFolders || workspaceFolders.length === 0) {
    window.showWarningMessage(
      'No workspace folder found. Using the provided translations directory as is.',
    )
    return configPath
  }

  const cwd = workspaceFolders[0].uri.fsPath
  const monoWorkspace = await getWorkspace({ cwd, includeRoot: true })

  if (!monoWorkspace) {
    window.showWarningMessage(
      'Could not determine workspace structure. Using the provided translations directory as is.',
    )
    return configPath
  }

  // Try to get the current file path
  let currentFilePath = window.activeTextEditor?.document.uri.fsPath

  // If no active editor, try to use the first workspace folder
  if (!currentFilePath) {
    currentFilePath = cwd
    console.log(
      'No active editor found. Using the first workspace folder for context.',
    )
  }

  const currentPackage = monoWorkspace
    .getPackages()
    .find(p => currentFilePath.startsWith(p.root))

  if (currentPackage) {
    return path.resolve(currentPackage.root, configPath)
  }
  else {
    window.showWarningMessage(
      'Current context is not in a known package. Using workspace root for translations directory.',
    )
    return path.resolve(cwd, configPath)
  }
}
