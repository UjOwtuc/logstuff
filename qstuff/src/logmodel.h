#ifndef LOGMODEL_H
#define LOGMODEL_H

#include <QAbstractTableModel>
#include <QVariantList>

class LogModel : public QAbstractTableModel
{
public:
	LogModel(const QStringList& columns, QObject* parent = nullptr);

	int columnCount(const QModelIndex& /*parent*/) const;
	int rowCount(const QModelIndex& /*parent*/) const;
	QVariant data(const QModelIndex & index, int role) const;
	QVariant headerData(int section, Qt::Orientation orientation, int role) const;

	void setLogs(const QVariantList& data);
	QVariant rowData(int row) const;
	void toggleColumn(const QString& name);
	void setColumns(const QStringList& columns);
	const QStringList& columns() const { return m_columns; }

private:
	QStringList m_columns;
	QVariantList m_data;
};

#endif // LOGMODEL_H
