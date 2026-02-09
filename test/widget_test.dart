import 'package:flutter_test/flutter_test.dart';
import 'package:flux_down/main.dart';
import 'package:flux_down/src/theme/theme_provider.dart';

void main() {
  testWidgets('FluxDown app smoke test', (WidgetTester tester) async {
    await tester.pumpWidget(FluxDownApp(themeProvider: ThemeProvider()));
    expect(find.text('FluxDown'), findsOneWidget);
  });
}
